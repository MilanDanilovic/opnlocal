#!/usr/bin/env bash
# Makes the Linux AppImage meet the licenses of the Ubuntu libraries linuxdeploy bundles into it
# (GTK, WebKitGTK, GStreamer and more; many LGPL, some parts GPL). It adds every bundled package's
# copyright file and a notice (usr/share/doc/opnlocal/BUNDLED-LIBRARIES.txt), rebuilds the AppImage
# in place, and writes <name>.sources.tar next to it: the Ubuntu source packages of those
# libraries, to publish on the same release page as the AppImage.
#   scripts/appimage-licenses.sh <path/to/opnlocal.AppImage>
# Run it on the Ubuntu machine that built the AppImage. Needs squashfs-tools, dpkg-dev and
# deb-src entries in apt's sources.
set -euo pipefail

appimage=$(realpath "$1")
name=$(basename "$appimage" .AppImage)
out=$(dirname "$appimage")
work=$(mktemp -d)
cd "$work"
chmod +x "$appimage"
"$appimage" --appimage-extract > /dev/null
root=squashfs-root

# The Ubuntu package of each bundled library. Everything else must be one of ours (listed below).
declare -A packages
while read -r lib; do
  base=$(basename "$lib")
  pkg=$(dpkg -S "*/$base" 2> /dev/null | head -1 | cut -d: -f1 | cut -d, -f1 || true)
  if [ -n "$pkg" ]; then
    packages[$pkg]=1
  else
    case "$base" in
      libggml* | libllama* | libvulkan.so*) ;;
      *) echo "Unknown bundled library $lib: find its license and add it to $0" >&2; exit 1 ;;
    esac
  fi
done < <(find "$root" -name "*.so*" -type f)
mapfile -t sorted < <(printf '%s\n' "${!packages[@]}" | sort)

for pkg in "${sorted[@]}"; do
  mkdir -p "$root/usr/share/doc/$pkg"
  cp -L "/usr/share/doc/$pkg/copyright" "$root/usr/share/doc/$pkg/copyright"
done

mkdir -p "$root/usr/share/doc/opnlocal"
cat > "$root/usr/share/doc/opnlocal/BUNDLED-LIBRARIES.txt" << EOF
opnlocal AppImage: bundled libraries
====================================

opnlocal is licensed under the Apache License 2.0. The open-source components built into it are
listed in THIRD_PARTY_NOTICES.txt, also shown in the app under Settings, About.

So that it runs on many Linux systems, this AppImage also bundles the libraries of the Ubuntu
22.04 packages listed below, unmodified. Each stays under its own license. Every package's
copyright and license file is in usr/share/doc/<package>/copyright inside this AppImage (run the
AppImage with --appimage-extract to see them).

Many of these libraries are licensed under the GNU LGPL, and some parts under the GNU GPL. You may
modify them and use your own versions: extract the AppImage, replace a library in usr/lib, and
start AppRun. You may also reverse engineer opnlocal to debug such modifications.

Source code: the source packages of these libraries, in exactly the versions bundled, are
published next to this AppImage on its release page as $name.sources.tar
(https://github.com/MilanDanilovic/opnlocal/releases). The source of opnlocal itself is at
https://github.com/MilanDanilovic/opnlocal.

Also bundled: opnlocal's llama.cpp libraries (libggml*, libllama*; MIT, see
THIRD_PARTY_NOTICES.txt) and the Khronos Vulkan loader from the LunarG Vulkan SDK
(libvulkan.so.1; Apache License 2.0).

Ubuntu packages (name, version):

$(dpkg-query -W -f='${Package} ${Version}\n' "${sorted[@]}")
EOF

mkdir sources
dpkg-query -W -f='${source:Package}=${source:Version}\n' "${sorted[@]}" | sort -u > sources/PACKAGES.txt
(cd sources && xargs -a PACKAGES.txt apt-get source --download-only -qq)
tar -cf "$out/$name.sources.tar" sources

# Rebuild: the original runtime (everything before the squashfs image) + a new image of the
# extracted tree, with the same compression.
offset=$("$appimage" --appimage-offset)
comp=$(unsquashfs -s -o "$offset" "$appimage" | sed -n 's/^Compression //p')
head -c "$offset" "$appimage" > "$name.AppImage"
mksquashfs "$root" image.squashfs -root-owned -noappend -comp "$comp" -quiet
cat image.squashfs >> "$name.AppImage"
chmod +x "$name.AppImage"
mv "$name.AppImage" "$appimage"
cd / && rm -rf "$work"
echo "$appimage: ${#sorted[@]} packages' licenses added; sources in $name.sources.tar"

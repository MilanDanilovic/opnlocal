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

# The Ubuntu package of each bundled file: files under usr/share by their exact path, the rest
# (libraries, typelibs, plugins; linuxdeploy flattens their folders) by name in Ubuntu's library
# folders only, since other apps on the machine (a browser in /opt, a CLI in /usr/lib/<name>)
# ship their own copies. Files from no package
# (AppRun, generated caches, opnlocal's own) are fine; libraries from no package must be ours.
declare -A packages
vulkan=""
while read -r file; do
  path=${file#"$root"}
  base=$(basename "$file")
  case "$base" in libggml* | libllama*) continue ;; esac # ours (the opnlocal .deb, if installed, is skipped too)
  case "$path" in
    /usr/share/*) found=$(dpkg -S "$path" 2> /dev/null || true) ;;
    *) found=$(dpkg -S "*/$base" 2> /dev/null | grep -E ': /(usr/)?lib/x86_64-linux-gnu/' || true) ;;
  esac
  # First Ubuntu-style match: not ours, and with a copyright file.
  pkg=""
  for candidate in $(echo "$found" | grep -v '^opnlocal:' | cut -d: -f1 | cut -d, -f1 || true); do
    if [ -f "/usr/share/doc/$candidate/copyright" ]; then pkg=$candidate; break; fi
  done
  if [ -n "$pkg" ]; then
    packages[$pkg]=1
  elif [[ $base == libvulkan.so* ]]; then
    vulkan=" and the Khronos Vulkan loader from the LunarG Vulkan SDK (libvulkan.so.1; Apache License 2.0)"
  elif [[ $base == *.so || $base == *.so.* ]]; then
    echo "Unknown bundled library $path: find its license and add it to $0" >&2
    exit 1
  fi
done < <(find "$root" -type f)
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

So that it runs on many Linux systems, this AppImage also bundles libraries and data files from
the Ubuntu 22.04 packages listed below, unmodified. Each stays under its own license. Every package's
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
THIRD_PARTY_NOTICES.txt)$vulkan.

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

#!/usr/bin/env bash
# Builds the Linux AppImage and .deb inside Ubuntu 22.04 (the oldest supported glibc), e.g.:
#   docker run --rm -v "$PWD":/src -w /src ubuntu:22.04 bash scripts/build-linux.sh
# Mirrors the `linux` job in .github/workflows/build.yml.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y curl build-essential pkg-config file patchelf xz-utils git \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libclang-dev libssl-dev \
  squashfs-tools dpkg-dev
# Source packages, for the AppImage's bundled libraries (scripts/appimage-licenses.sh).
for s in jammy jammy-updates jammy-security; do
  echo "deb-src http://archive.ubuntu.com/ubuntu $s main restricted universe multiverse"
done > /etc/apt/sources.list.d/opnlocal-src.list
apt-get update

if ! command -v cargo >/dev/null; then
  curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  . "$HOME/.cargo/env"
fi
if ! command -v node >/dev/null; then
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
  apt-get install -y nodejs
fi
command -v cmake >/dev/null || { apt-get install -y python3-pip && pip3 install cmake ninja; }

V=1.4.321.1
if [ ! -d "/opt/vulkan/$V" ]; then
  mkdir -p /opt/vulkan
  curl -sSL "https://sdk.lunarg.com/sdk/download/$V/linux/vulkansdk-linux-x86_64-$V.tar.xz" | tar -xJ -C /opt/vulkan
fi
export VULKAN_SDK="/opt/vulkan/$V/x86_64"
export PATH="$VULKAN_SDK/bin:$PATH"
# Separate target dir so a Windows checkout's target/ isn't reused.
export CARGO_TARGET_DIR=/src/target/linux

cd app
npm ci
export LD_LIBRARY_PATH="$PWD/src-tauri/libs:${LD_LIBRARY_PATH:-}"
export NO_STRIP=true
export APPIMAGE_EXTRACT_AND_RUN=1   # no FUSE inside containers
npx tauri build --bundles appimage,deb
bash ../scripts/appimage-licenses.sh "$CARGO_TARGET_DIR"/release/bundle/appimage/*.AppImage
ls -la "$CARGO_TARGET_DIR/release/bundle/appimage" "$CARGO_TARGET_DIR/release/bundle/deb"

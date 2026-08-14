#!/usr/bin/env bash
set -euo pipefail

readonly libtorrent_version="2.1.1"
readonly archive="libtorrent-rasterbar-${libtorrent_version}.tar.gz"
readonly archive_sha256="0f163516ecef2e3331500266751de3098835a3c3ae0c2290448046c632bc0e93"
readonly source_url="https://github.com/arvidn/libtorrent/releases/download/v${libtorrent_version}/${archive}"

build_root="$(mktemp -d)"
trap 'rm -rf "${build_root}"' EXIT

curl --fail --location --retry 5 --output "${build_root}/${archive}" "${source_url}"
echo "${archive_sha256}  ${build_root}/${archive}" | sha256sum --check --strict
tar --extract --gzip --file "${build_root}/${archive}" --directory "${build_root}"

cmake \
  -S "${build_root}/libtorrent-rasterbar-${libtorrent_version}" \
  -B "${build_root}/build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX=/usr/local \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DBUILD_SHARED_LIBS=OFF \
  -Ddeprecated-functions=OFF \
  -Dwebtorrent=OFF \
  -Dbuild_tests=OFF \
  -Dbuild_examples=OFF \
  -Dbuild_tools=OFF \
  -Dpython-bindings=OFF
cmake --build "${build_root}/build" --parallel "$(nproc)"
if command -v sudo >/dev/null 2>&1; then
  sudo cmake --install "${build_root}/build"
else
  cmake --install "${build_root}/build"
fi

test "$(PKG_CONFIG_PATH=/usr/local/lib/pkgconfig pkg-config --modversion libtorrent-rasterbar)" = "${libtorrent_version}"

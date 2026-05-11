#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET="${AURALUX_WINDOWS_TARGET:-x86_64-pc-windows-gnu}"
ARCHIVE_NAME="${AURALUX_WINDOWS_ARCHIVE_NAME:-auralux-windows-x86_64}"
PROXY_URL="${AURALUX_PROXY:-http://127.0.0.1:19000}"
RUSTUP_MIRROR="${AURALUX_RUSTUP_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn/rustup}"
NPM_REGISTRY="${AURALUX_NPM_REGISTRY:-https://registry.npmmirror.com}"
ZIG_VERSION="${AURALUX_ZIG_VERSION:-0.13.0}"
ZIG_NPM_PACKAGE="${AURALUX_ZIG_NPM_PACKAGE:-@ryoppippi/zig-linux-x64@${ZIG_VERSION}}"
BUILD_DIR="$ROOT_DIR/.auralux-build"
BIN_DIR="$BUILD_DIR/bin"
ZIG_DIR="$BUILD_DIR/zig"
APT_DOWNLOAD_DIR="$BUILD_DIR/downloads/apt"
MINGW_DIR="$BUILD_DIR/mingw"
MINGW_BIN="$MINGW_DIR/usr/bin"
MINGW_HOST_DIR="$BUILD_DIR/mingw-host"
MINGW_HOST_LIB="$MINGW_HOST_DIR/usr/lib/x86_64-linux-gnu"
MINGW_APT_PACKAGES=(
  mingw-w64-common
  mingw-w64-x86-64-dev
  binutils-mingw-w64-x86-64
  gcc-mingw-w64-base
  gcc-mingw-w64-x86-64-posix-runtime
  gcc-mingw-w64-x86-64-posix
  g++-mingw-w64-x86-64-posix
)
MINGW_HOST_APT_PACKAGES=(
  libisl23
  libmpfr6
  libmpc3
)

log() {
  printf '[auralux-win] %s\n' "$*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

is_mirror_url() {
  case "$1" in
    https://mirrors.tuna.tsinghua.edu.cn/*|http://mirrors.tuna.tsinghua.edu.cn/*|\
https://mirrors.ustc.edu.cn/*|http://mirrors.ustc.edu.cn/*|\
https://mirrors.cloud.tencent.com/*|http://mirrors.cloud.tencent.com/*|\
https://mirrors.huaweicloud.com/*|http://mirrors.huaweicloud.com/*|\
https://registry.npmmirror.com/*|http://registry.npmmirror.com/*|\
https://rsproxy.cn/*|http://rsproxy.cn/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

without_proxy() {
  env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy "$@"
}

run_npm() {
  if is_mirror_url "$NPM_REGISTRY"; then
    without_proxy npm "$@"
  else
    npm "$@"
  fi
}

configure_proxy() {
  export HTTP_PROXY="${HTTP_PROXY:-$PROXY_URL}"
  export HTTPS_PROXY="${HTTPS_PROXY:-$PROXY_URL}"
  export http_proxy="${http_proxy:-$HTTP_PROXY}"
  export https_proxy="${https_proxy:-$HTTPS_PROXY}"
  export RUSTUP_DIST_SERVER="${RUSTUP_DIST_SERVER:-$RUSTUP_MIRROR}"
  export RUSTUP_UPDATE_ROOT="${RUSTUP_UPDATE_ROOT:-$RUSTUP_MIRROR/rustup}"

  npm config set registry "$NPM_REGISTRY" >/dev/null
  if is_mirror_url "$NPM_REGISTRY"; then
    npm config delete proxy >/dev/null 2>&1 || true
    npm config delete https-proxy >/dev/null 2>&1 || true
  else
    npm config set proxy "$HTTP_PROXY" >/dev/null
    npm config set https-proxy "$HTTPS_PROXY" >/dev/null
  fi
}

install_zig() {
  mkdir -p "$BIN_DIR" "$ZIG_DIR"
  if [ -x "$ZIG_DIR/zig" ]; then
    log "Using cached Zig: $($ZIG_DIR/zig version)"
  else
    log "Installing Zig $ZIG_VERSION via npm mirror package $ZIG_NPM_PACKAGE"
    local pack_output tgz
    pack_output="$(run_npm pack "$ZIG_NPM_PACKAGE" --pack-destination "$BUILD_DIR")"
    tgz="$BUILD_DIR/$(printf '%s\n' "$pack_output" | tail -n 1)"
    rm -rf "$ZIG_DIR"
    mkdir -p "$ZIG_DIR"
    tar -xzf "$tgz" -C "$ZIG_DIR" --strip-components=1
    chmod +x "$ZIG_DIR/zig"
    log "Installed Zig: $($ZIG_DIR/zig version)"
  fi
}

apt_source_urls() {
  if [ -f /etc/apt/sources.list ]; then
    awk '$1 !~ /^#/ && ($1 == "deb" || $1 == "deb-src") { for (i = 2; i <= NF; i++) if ($i ~ /^https?:\/\//) { print $i; break } }' /etc/apt/sources.list
  fi

  if [ -d /etc/apt/sources.list.d ]; then
    find /etc/apt/sources.list.d -maxdepth 1 -type f \( -name '*.list' -o -name '*.sources' \) -print0 2>/dev/null |
      while IFS= read -r -d '' file; do
        awk '
          $1 !~ /^#/ && ($1 == "deb" || $1 == "deb-src") {
            for (i = 2; i <= NF; i++) if ($i ~ /^https?:\/\//) { print $i; break }
          }
          $1 == "URIs:" {
            for (i = 2; i <= NF; i++) if ($i ~ /^https?:\/\//) print $i
          }
        ' "$file"
      done
  fi
}

apt_should_use_proxy() {
  if [ "${AURALUX_APT_DIRECT:-0}" = "1" ]; then
    return 1
  fi
  if [ "${AURALUX_APT_USE_PROXY:-}" = "1" ]; then
    return 0
  fi
  if [ "${AURALUX_APT_USE_PROXY:-}" = "0" ]; then
    return 1
  fi

  local url saw_url=0
  while IFS= read -r url; do
    [ -z "$url" ] && continue
    saw_url=1
    if ! is_mirror_url "$url"; then
      return 0
    fi
  done < <(apt_source_urls)

  [ "$saw_url" -eq 0 ] && return 0
  return 1
}

download_apt_package() {
  local package="$1"
  mkdir -p "$APT_DOWNLOAD_DIR"
  if compgen -G "$APT_DOWNLOAD_DIR/${package}_*.deb" >/dev/null; then
    log "Using cached apt package: $package"
    return
  fi

  log "Downloading apt package: $package"
  if apt_should_use_proxy; then
    (
      cd "$APT_DOWNLOAD_DIR"
      apt-get \
        -o Acquire::http::Proxy="$HTTP_PROXY" \
        -o Acquire::https::Proxy="$HTTPS_PROXY" \
        download "$package"
    )
  else
    (
      cd "$APT_DOWNLOAD_DIR"
      without_proxy apt-get \
        -o Acquire::http::Proxy="DIRECT" \
        -o Acquire::https::Proxy="DIRECT" \
        download "$package"
    )
  fi
}

write_mingw_shims() {
  mkdir -p "$BIN_DIR"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-gcc-posix" "$BIN_DIR/x86_64-w64-mingw32-gcc"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-g++-posix" "$BIN_DIR/x86_64-w64-mingw32-g++"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-g++-posix" "$BIN_DIR/x86_64-w64-mingw32-c++"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-gcc-posix" "$BIN_DIR/gcc"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-gcc-posix" "$BIN_DIR/cc"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-g++-posix" "$BIN_DIR/g++"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-g++-posix" "$BIN_DIR/c++"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-dlltool" "$BIN_DIR/x86_64-w64-mingw32-dlltool"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-dlltool" "$BIN_DIR/dlltool"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-windres" "$BIN_DIR/x86_64-w64-mingw32-windres"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-ar" "$BIN_DIR/x86_64-w64-mingw32-ar"
  ln -sf "$MINGW_BIN/x86_64-w64-mingw32-ranlib" "$BIN_DIR/x86_64-w64-mingw32-ranlib"
}

ensure_mingw() {
  if [ -x "$MINGW_BIN/x86_64-w64-mingw32-gcc-posix" ] &&
    [ -x "$MINGW_BIN/x86_64-w64-mingw32-g++-posix" ] &&
    [ -x "$MINGW_BIN/x86_64-w64-mingw32-dlltool" ] &&
    [ -x "$MINGW_BIN/x86_64-w64-mingw32-windres" ] &&
    [ -f "$MINGW_HOST_LIB/libisl.so.23" ] &&
    [ -f "$MINGW_HOST_LIB/libmpfr.so.6" ] &&
    [ -f "$MINGW_HOST_LIB/libmpc.so.3" ]; then
    log "Using cached MinGW toolchain: $($MINGW_BIN/x86_64-w64-mingw32-gcc-posix -dumpversion)"
    write_mingw_shims
    return
  fi

  require_cmd apt-get
  require_cmd dpkg-deb

  local package
  for package in "${MINGW_APT_PACKAGES[@]}"; do
    download_apt_package "$package"
  done
  for package in "${MINGW_HOST_APT_PACKAGES[@]}"; do
    download_apt_package "$package"
  done

  log "Extracting MinGW toolchain into ${MINGW_DIR#$ROOT_DIR/}"
  rm -rf "$MINGW_DIR.tmp"
  mkdir -p "$MINGW_DIR.tmp"
  for package in "${MINGW_APT_PACKAGES[@]}"; do
    for deb in "$APT_DOWNLOAD_DIR"/"${package}"_*.deb; do
      dpkg-deb -x "$deb" "$MINGW_DIR.tmp"
    done
  done
  rm -rf "$MINGW_DIR"
  mv "$MINGW_DIR.tmp" "$MINGW_DIR"

  log "Extracting MinGW host runtime libraries into ${MINGW_HOST_DIR#$ROOT_DIR/}"
  rm -rf "$MINGW_HOST_DIR.tmp"
  mkdir -p "$MINGW_HOST_DIR.tmp"
  for package in "${MINGW_HOST_APT_PACKAGES[@]}"; do
    for deb in "$APT_DOWNLOAD_DIR"/"${package}"_*.deb; do
      dpkg-deb -x "$deb" "$MINGW_HOST_DIR.tmp"
    done
  done
  rm -rf "$MINGW_HOST_DIR"
  mv "$MINGW_HOST_DIR.tmp" "$MINGW_HOST_DIR"

  if ! [ -f "$MINGW_HOST_LIB/libisl.so.23" ]; then
    printf 'MinGW host runtime extraction failed: missing %s\n' "$MINGW_HOST_LIB/libisl.so.23" >&2
    exit 1
  fi
  write_mingw_shims
}

write_tool_wrappers() {
  cat > "$BIN_DIR/zig-cc" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
zig_bin="${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}"
args=()
for arg in "$@"; do
  case "$arg" in
    --target=x86_64-unknown-linux-gnu) arg=--target=x86_64-linux-gnu ;;
    -target=x86_64-unknown-linux-gnu) arg=-target=x86_64-linux-gnu ;;
    x86_64-unknown-linux-gnu) arg=x86_64-linux-gnu ;;
    --target=x86_64-pc-windows-gnu) arg=--target=x86_64-windows-gnu ;;
    -target=x86_64-pc-windows-gnu) arg=-target=x86_64-windows-gnu ;;
    x86_64-pc-windows-gnu) arg=x86_64-windows-gnu ;;
  esac
  args+=("$arg")
done
exec "$zig_bin" cc "${args[@]}"
WRAPPER

  cat > "$BIN_DIR/zig-cc-windows" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
zig_bin="${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}"
args=(--target=x86_64-windows-gnu)
skip_next=0
for arg in "$@"; do
  if [ "$skip_next" -eq 1 ]; then
    skip_next=0
    continue
  fi
  case "$arg" in
    --target=*|-target=*)
      ;;
    -target)
      skip_next=1
      ;;
    x86_64-pc-windows-gnu)
      ;;
    *)
      args+=("$arg")
      ;;
  esac
done
exec "$zig_bin" cc "${args[@]}"
WRAPPER

  cat > "$BIN_DIR/zig-cxx" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
zig_bin="${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}"
args=()
for arg in "$@"; do
  case "$arg" in
    --target=x86_64-unknown-linux-gnu) arg=--target=x86_64-linux-gnu ;;
    -target=x86_64-unknown-linux-gnu) arg=-target=x86_64-linux-gnu ;;
    x86_64-unknown-linux-gnu) arg=x86_64-linux-gnu ;;
    --target=x86_64-pc-windows-gnu) arg=--target=x86_64-windows-gnu ;;
    -target=x86_64-pc-windows-gnu) arg=-target=x86_64-windows-gnu ;;
    x86_64-pc-windows-gnu) arg=x86_64-windows-gnu ;;
  esac
  args+=("$arg")
done
exec "$zig_bin" c++ "${args[@]}"
WRAPPER

  cat > "$BIN_DIR/zig-cxx-windows" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
zig_bin="${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}"
args=(--target=x86_64-windows-gnu)
skip_next=0
for arg in "$@"; do
  if [ "$skip_next" -eq 1 ]; then
    skip_next=0
    continue
  fi
  case "$arg" in
    --target=*|-target=*)
      ;;
    -target)
      skip_next=1
      ;;
    x86_64-pc-windows-gnu)
      ;;
    *)
      args+=("$arg")
      ;;
  esac
done
exec "$zig_bin" c++ "${args[@]}"
WRAPPER

  cat > "$BIN_DIR/zig-ar" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
exec "${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}" ar "$@"
WRAPPER

  cat > "$BIN_DIR/zig-ranlib" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
exec "${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}" ranlib "$@"
WRAPPER

  cat > "$BIN_DIR/zig-dlltool" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
exec "${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}" dlltool "$@"
WRAPPER

  cat > "$BIN_DIR/zig-windres" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
zig_bin="${AURALUX_ZIG_BIN:?AURALUX_ZIG_BIN is required}"
args=()
output=""
input=""

for arg in "$@"; do
  case "$arg" in
    -V|/?|--version|--help)
      printf 'GNU windres (Auralux Zig rc wrapper) 2.40\n'
      exit 0
      ;;
  esac
done

while [ "$#" -gt 0 ]; do
  case "$1" in
    --input)
      input="$2"
      shift 2
      ;;
    --output)
      output="$2"
      shift 2
      ;;
    --include-dir)
      args+=("/i" "$2")
      shift 2
      ;;
    --output-format|--target|-c)
      shift 2
      ;;
    -D)
      args+=("/d" "$2")
      shift 2
      ;;
    -D*)
      args+=("/d" "${1#-D}")
      shift
      ;;
    -I)
      args+=("/i" "$2")
      shift 2
      ;;
    -I*)
      args+=("/i" "${1#-I}")
      shift
      ;;
    *)
      shift
      ;;
  esac
done

if [ -z "$input" ] || [ -z "$output" ]; then
  printf 'zig-windres expected --input and --output arguments\n' >&2
  exit 1
fi

exec "$zig_bin" rc "/:auto-includes" "gnu" "/fo" "$output" "/c" "65001" "${args[@]}" -- "$input"
WRAPPER

  chmod +x "$BIN_DIR"/zig-*
  ln -sf zig-dlltool "$BIN_DIR/x86_64-w64-mingw32-dlltool"
  ln -sf zig-dlltool "$BIN_DIR/dlltool"
  ln -sf zig-windres "$BIN_DIR/x86_64-w64-mingw32-windres"
  ln -sf zig-cc-windows "$BIN_DIR/x86_64-w64-mingw32-gcc"
  ln -sf zig-cxx-windows "$BIN_DIR/x86_64-w64-mingw32-g++"
}

install_windows_target() {
  if rustup target list --installed | grep -qx "$TARGET"; then
    log "Rust target already installed: $TARGET"
    return
  fi

  log "Installing Rust target: $TARGET"
  local mirrors=(
    "$RUSTUP_DIST_SERVER"
    "https://static.rust-lang.org"
  )
  local mirror
  for mirror in "${mirrors[@]}"; do
    log "Trying rustup mirror: $mirror"
    if is_mirror_url "$mirror"; then
      if without_proxy RUSTUP_DIST_SERVER="$mirror" RUSTUP_UPDATE_ROOT="$mirror/rustup" rustup target add "$TARGET"; then
        return
      fi
    elif RUSTUP_DIST_SERVER="$mirror" RUSTUP_UPDATE_ROOT="$mirror/rustup" rustup target add "$TARGET"; then
      return
    fi
  done

  printf 'Failed to install Rust target %s. Check proxy or mirror availability.\n' "$TARGET" >&2
  exit 1
}

build_frontend() {
  if [ ! -d node_modules ]; then
    log "Installing npm dependencies"
    run_npm install
  fi
  log "Building shared GUI"
  run_npm --workspace apps/gui run build
}

ensure_windows_icon() {
  local icon_path="$ROOT_DIR/apps/tauri/src-tauri/icons/icon.ico"
  if [ -f "$icon_path" ]; then
    log "Using existing Windows icon: ${icon_path#$ROOT_DIR/}"
    return
  fi

  require_cmd node
  log "Generating Windows icon.ico from PNG assets"
  node <<'NODE'
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';

const root = process.cwd();
const iconDir = path.join(root, 'apps/tauri/src-tauri/icons');
const source = path.join(iconDir, 'icon.png');
const output = path.join(iconDir, 'icon.ico');
const sizes = [256, 128, 64, 48, 32, 16];

const images = await Promise.all(
  sizes.map(async (size) => ({
    size,
    data: await sharp(source)
      .resize(size, size, { fit: 'contain' })
      .png()
      .toBuffer(),
  })),
);

const headerSize = 6 + images.length * 16;
let offset = headerSize;
const header = Buffer.alloc(headerSize);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(images.length, 4);

for (const [index, image] of images.entries()) {
  const pos = 6 + index * 16;
  header.writeUInt8(image.size === 256 ? 0 : image.size, pos);
  header.writeUInt8(image.size === 256 ? 0 : image.size, pos + 1);
  header.writeUInt8(0, pos + 2);
  header.writeUInt8(0, pos + 3);
  header.writeUInt16LE(1, pos + 4);
  header.writeUInt16LE(32, pos + 6);
  header.writeUInt32LE(image.data.length, pos + 8);
  header.writeUInt32LE(offset, pos + 12);
  offset += image.data.length;
}

await mkdir(iconDir, { recursive: true });
await writeFile(output, Buffer.concat([header, ...images.map((image) => image.data)]));
NODE
}

build_tauri_exe() {
  export PATH="$BIN_DIR:$MINGW_BIN:$PATH"
  export LD_LIBRARY_PATH="$MINGW_HOST_LIB${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export CC_x86_64_pc_windows_gnu="$MINGW_BIN/x86_64-w64-mingw32-gcc-posix"
  export CXX_x86_64_pc_windows_gnu="$MINGW_BIN/x86_64-w64-mingw32-g++-posix"
  export AR_x86_64_pc_windows_gnu="$MINGW_BIN/x86_64-w64-mingw32-ar"
  export RANLIB_x86_64_pc_windows_gnu="$MINGW_BIN/x86_64-w64-mingw32-ranlib"
  export RC_x86_64_pc_windows_gnu="$MINGW_BIN/x86_64-w64-mingw32-windres"
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$MINGW_BIN/x86_64-w64-mingw32-gcc-posix"
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR="$MINGW_BIN/x86_64-w64-mingw32-ar"
  export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-Cdlltool=$MINGW_BIN/x86_64-w64-mingw32-dlltool ${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS:-}"

  log "Building Windows desktop executable with target $TARGET"
  run_npm --workspace apps/tauri exec tauri -- build --target "$TARGET" --no-bundle --ci
}

stage_artifacts() {
  local stage_dir="$ROOT_DIR/release/$ARCHIVE_NAME"
  rm -rf "$stage_dir"
  mkdir -p "$stage_dir"

  local exe_path="target/$TARGET/release/auralux-tauri.exe"
  if [ ! -f "$exe_path" ]; then
    exe_path="apps/tauri/src-tauri/target/$TARGET/release/auralux-tauri.exe"
  fi
  cp "$exe_path" "$stage_dir/Auralux.exe"
  if [ -f "$(dirname "$exe_path")/WebView2Loader.dll" ]; then
    cp "$(dirname "$exe_path")/WebView2Loader.dll" "$stage_dir/"
  fi
  cp README.md README.zh-CN.md LICENSE NOTICE "$stage_dir/"

  if [ -d apps/gui/dist ]; then
    mkdir -p "$stage_dir/web"
    cp -R apps/gui/dist/. "$stage_dir/web/"
  fi

  (
    cd "$ROOT_DIR/release"
    rm -f "$ARCHIVE_NAME.zip" "$ARCHIVE_NAME.zip.sha256"
    if command -v zip >/dev/null 2>&1; then
      zip -qr "$ARCHIVE_NAME.zip" "$ARCHIVE_NAME"
    else
      run_npm exec --yes bestzip -- "$ARCHIVE_NAME.zip" "$ARCHIVE_NAME" >/dev/null
    fi
    sha256sum "$ARCHIVE_NAME.zip" > "$ARCHIVE_NAME.zip.sha256"
  )

  log "Windows desktop artifact: release/$ARCHIVE_NAME/Auralux.exe"
  log "Archive: release/$ARCHIVE_NAME.zip"
}

main() {
  require_cmd npm
  require_cmd rustup
  require_cmd cargo
  require_cmd tar
  require_cmd sha256sum

  configure_proxy
  ensure_mingw
  install_windows_target
  build_frontend
  ensure_windows_icon
  build_tauri_exe
  stage_artifacts
}

main "$@"

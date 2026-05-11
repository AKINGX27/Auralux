#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PROXY_URL="${AURALUX_PROXY:-http://127.0.0.1:19000}"
RUSTUP_MIRROR="${AURALUX_RUSTUP_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn/rustup}"
NPM_REGISTRY="${AURALUX_NPM_REGISTRY:-https://registry.npmmirror.com}"
JDK_URLS_DEFAULT="https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/x64/linux/OpenJDK21U-jdk_x64_linux_hotspot_21.0.11_10.tar.gz https://api.adoptium.net/v3/binary/latest/21/ga/linux/x64/jdk/hotspot/normal/eclipse"
JDK_URLS="${AURALUX_JDK_URLS:-${AURALUX_JDK_URL:-$JDK_URLS_DEFAULT}}"
ANDROID_CMDLINE_TOOLS_URL="${AURALUX_ANDROID_CMDLINE_TOOLS_URL:-https://dl.google.com/android/repository/commandlinetools-linux-13114758_latest.zip}"
ANDROID_API="${AURALUX_ANDROID_API:-35}"
ANDROID_PLATFORM_URL="${AURALUX_ANDROID_PLATFORM_URL:-https://dl.google.com/android/repository/platform-35_r01.zip}"
ANDROID_BUILD_TOOLS="${AURALUX_ANDROID_BUILD_TOOLS:-35.0.0}"
ANDROID_BUILD_TOOLS_URL="${AURALUX_ANDROID_BUILD_TOOLS_URL:-https://dl.google.com/android/repository/build-tools_r35_linux.zip}"
ANDROID_PLATFORM_TOOLS_VERSION="${AURALUX_ANDROID_PLATFORM_TOOLS_VERSION:-35.0.2}"
ANDROID_PLATFORM_TOOLS_URL="${AURALUX_ANDROID_PLATFORM_TOOLS_URL:-https://dl.google.com/android/repository/platform-tools_r35.0.2-linux.zip}"
ANDROID_NDK_VERSION="${AURALUX_ANDROID_NDK_VERSION:-27.2.12479018}"
ANDROID_NDK_URL="${AURALUX_ANDROID_NDK_URL:-https://dl.google.com/android/repository/android-ndk-r27c-linux.zip}"
ANDROID_CMAKE_VERSION="${AURALUX_ANDROID_CMAKE_VERSION:-3.22.1}"
ANDROID_CMAKE_URL="${AURALUX_ANDROID_CMAKE_URL:-https://dl.google.com/android/repository/cmake-3.22.1-linux.zip}"
ANDROID_ABI="${AURALUX_ANDROID_ABI:-aarch64}"
GRADLE_VERSION="${AURALUX_GRADLE_VERSION:-8.14.3}"
GRADLE_URLS_DEFAULT="https://mirrors.cloud.tencent.com/gradle/gradle-${GRADLE_VERSION}-bin.zip https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip"
GRADLE_URLS="${AURALUX_GRADLE_URLS:-${AURALUX_GRADLE_URL:-$GRADLE_URLS_DEFAULT}}"
GRADLE_MAVEN_MIRRORS="${AURALUX_GRADLE_MAVEN_MIRRORS:-https://maven.aliyun.com/repository/google https://maven.aliyun.com/repository/public https://maven.aliyun.com/repository/central https://maven.aliyun.com/repository/gradle-plugin https://maven.aliyun.com/repository/jcenter https://mirrors.cloud.tencent.com/nexus/repository/maven-public/ https://repo.huaweicloud.com/repository/maven/}"
GRADLE_OFFICIAL_REPOSITORIES="${AURALUX_GRADLE_OFFICIAL_REPOSITORIES:-0}"
GRADLE_NON_PROXY_HOSTS="${AURALUX_GRADLE_NON_PROXY_HOSTS:-localhost|127.*|[::1]|maven.aliyun.com|*.aliyun.com|mirrors.cloud.tencent.com|mirrors.tencent.com|repo.huaweicloud.com|mirrors.huaweicloud.com|mirrors.tuna.tsinghua.edu.cn|mirrors.ustc.edu.cn|registry.npmmirror.com|rsproxy.cn}"
BUILD_DIR="$ROOT_DIR/.auralux-build"
DOWNLOAD_DIR="$BUILD_DIR/downloads"
NPM_CACHE_DIR="$BUILD_DIR/npm-cache"
JDK_DIR="$BUILD_DIR/jdk"
ANDROID_HOME_DIR="$BUILD_DIR/android-sdk"
GRADLE_USER_HOME_DIR="$BUILD_DIR/gradle"

log() {
  printf '[auralux-apk] %s\n' "$*"
}

usage() {
  cat <<EOF
Usage: npm run build:android:apk

Builds the Auralux Android APK and stages artifacts in release/auralux-android.

Useful environment variables:
  AURALUX_PROXY=http://127.0.0.1:19000
  AURALUX_ANDROID_ABI=aarch64
  AURALUX_ANDROID_SDK_DIRECT=1
  AURALUX_GRADLE_OFFICIAL_REPOSITORIES=1
  AURALUX_GRADLE_MAVEN_MIRRORS="https://maven.aliyun.com/repository/google ..."

Caches kept under .auralux-build:
  downloads/     JDK, Android SDK package zips, Gradle distribution
  android-sdk/   extracted Android SDK, build-tools, platform, CMake, NDK
  gradle/        Gradle wrapper, Maven dependency cache, mirror config
  jdk/           extracted JDK 21
  npm-cache/     npm package cache
EOF
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

download() {
  local url="$1"
  local out="$2"
  if [ -s "$out" ]; then
    log "Using cached download: $out"
    return
  fi

  mkdir -p "$(dirname "$out")"
  log "Downloading $url"
  rm -f "$out"
  if is_direct_download_url "$url"; then
    log "Direct-download URL detected, downloading without proxy"
    env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy \
      wget --tries=3 --continue --progress=dot:giga -O "$out" "$url"
  else
    wget --tries=3 --continue --progress=dot:giga -O "$out" "$url"
  fi
}

download_first() {
  local out="$1"
  shift
  local url
  for url in "$@"; do
    if download "$url" "$out"; then
      return
    fi
    rm -f "$out"
    log "Download failed, trying next source"
  done

  printf 'All download sources failed for %s\n' "$out" >&2
  exit 1
}

apt_download_proxy() {
  if [ "${AURALUX_APT_DIRECT:-0}" = "1" ]; then
    env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy apt-get "$@"
  else
    apt-get \
      -o "Acquire::http::Proxy=$HTTP_PROXY" \
      -o "Acquire::https::Proxy=$HTTPS_PROXY" \
      "$@"
  fi
}

is_mirror_url() {
  case "$1" in
    https://mirrors.tuna.tsinghua.edu.cn/*|http://mirrors.tuna.tsinghua.edu.cn/*|\
https://mirrors.ustc.edu.cn/*|http://mirrors.ustc.edu.cn/*|\
https://mirrors.cloud.tencent.com/*|http://mirrors.cloud.tencent.com/*|\
https://mirrors.huaweicloud.com/*|http://mirrors.huaweicloud.com/*|\
https://registry.npmmirror.com/*|http://registry.npmmirror.com/*|\
https://maven.aliyun.com/*|http://maven.aliyun.com/*|\
https://repo.huaweicloud.com/*|http://repo.huaweicloud.com/*|\
https://mirrors.tencent.com/*|http://mirrors.tencent.com/*|\
https://rsproxy.cn/*|http://rsproxy.cn/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

is_direct_download_url() {
  if is_mirror_url "$1"; then
    return 0
  fi

  case "$1" in
    https://dl.google.com/android/repository/*)
      [ "${AURALUX_ANDROID_CMDLINE_DIRECT:-1}" = "1" ]
      ;;
    *)
      return 1
      ;;
  esac
}

configure_proxy() {
  mkdir -p "$DOWNLOAD_DIR" "$NPM_CACHE_DIR" "$GRADLE_USER_HOME_DIR"

  export HTTP_PROXY="${HTTP_PROXY:-$PROXY_URL}"
  export HTTPS_PROXY="${HTTPS_PROXY:-$PROXY_URL}"
  export http_proxy="${http_proxy:-$HTTP_PROXY}"
  export https_proxy="${https_proxy:-$HTTPS_PROXY}"
  export RUSTUP_DIST_SERVER="${RUSTUP_DIST_SERVER:-$RUSTUP_MIRROR}"
  export RUSTUP_UPDATE_ROOT="${RUSTUP_UPDATE_ROOT:-$RUSTUP_MIRROR/rustup}"
  export NPM_CONFIG_CACHE="${NPM_CONFIG_CACHE:-$NPM_CACHE_DIR}"

  npm config set registry "$NPM_REGISTRY" >/dev/null
  npm config set cache "$NPM_CONFIG_CACHE" >/dev/null
  npm config set proxy "$HTTP_PROXY" >/dev/null
  npm config set https-proxy "$HTTPS_PROXY" >/dev/null
  npm config set noproxy "registry.npmmirror.com,mirrors.tuna.tsinghua.edu.cn,mirrors.ustc.edu.cn,mirrors.cloud.tencent.com,mirrors.huaweicloud.com,rsproxy.cn" >/dev/null
}

sdkmanager_proxy_args() {
  if [ "${AURALUX_ANDROID_SDK_DIRECT:-0}" = "1" ]; then
    return
  fi
  printf '%s\n' "--proxy=http" "--proxy_host=127.0.0.1" "--proxy_port=19000"
}

install_jdk() {
  if [ -x "$JDK_DIR/bin/java" ]; then
    log "Using cached JDK: $("$JDK_DIR/bin/java" -version 2>&1 | head -n 1)"
    return
  fi

  local archive="$DOWNLOAD_DIR/jdk-21-linux-x64.tar.gz"
  # shellcheck disable=SC2086
  download_first "$archive" $JDK_URLS
  rm -rf "$JDK_DIR"
  mkdir -p "$JDK_DIR"
  tar -xzf "$archive" -C "$JDK_DIR" --strip-components=1
  log "Installed JDK: $("$JDK_DIR/bin/java" -version 2>&1 | head -n 1)"
}

install_android_cmdline_tools() {
  export JAVA_HOME="$JDK_DIR"
  export PATH="$JAVA_HOME/bin:$PATH"

  if [ -x "$ANDROID_HOME_DIR/cmdline-tools/latest/bin/sdkmanager" ]; then
    log "Using cached Android command-line tools"
    return
  fi

  local archive="$DOWNLOAD_DIR/android-commandlinetools.zip"
  download "$ANDROID_CMDLINE_TOOLS_URL" "$archive"
  rm -rf "$ANDROID_HOME_DIR/cmdline-tools"
  mkdir -p "$ANDROID_HOME_DIR/cmdline-tools/latest"

  local unpack_dir="$BUILD_DIR/android-cmdline-tools-unpack"
  rm -rf "$unpack_dir"
  mkdir -p "$unpack_dir"
  (cd "$unpack_dir" && jar xf "$archive")
  cp -R "$unpack_dir/cmdline-tools/." "$ANDROID_HOME_DIR/cmdline-tools/latest/"
  chmod +x "$ANDROID_HOME_DIR/cmdline-tools/latest/bin/"*
}

install_android_packages() {
  export JAVA_HOME="$JDK_DIR"
  export ANDROID_HOME="$ANDROID_HOME_DIR"
  export ANDROID_SDK_ROOT="$ANDROID_HOME_DIR"
  export GRADLE_USER_HOME="$GRADLE_USER_HOME_DIR"
  export PATH="$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

  mkdir -p "$ANDROID_HOME" "$GRADLE_USER_HOME"
  mapfile -t sdk_proxy_args < <(sdkmanager_proxy_args)
  yes | sdkmanager "${sdk_proxy_args[@]}" --licenses >/dev/null || true
  install_android_platform_tools_cache
  install_android_platform_cache
  install_android_build_tools_cache
  install_android_cmake_cache
  install_android_ndk_cache

  local packages=()
  local missing=()
  local package
  local installed="$BUILD_DIR/sdkmanager-installed.txt"
  sdkmanager --list_installed > "$installed" || true
  for package in "${packages[@]}"; do
    if ! grep -Fq "$package" "$installed"; then
      missing+=("$package")
    fi
  done

  if [ "${#missing[@]}" -eq 0 ]; then
    log "Android SDK packages already installed in cache"
    return
  fi

  log "Installing Android SDK packages: ${missing[*]}"
  sdkmanager "${sdk_proxy_args[@]}" "${missing[@]}"
}

install_android_platform_tools_cache() {
  local tools_dir="$ANDROID_HOME_DIR/platform-tools"
  if [ -x "$tools_dir/adb" ]; then
    log "Using cached Android platform-tools"
    return
  fi

  local archive="$DOWNLOAD_DIR/platform-tools_r$ANDROID_PLATFORM_TOOLS_VERSION-linux.zip"
  download "$ANDROID_PLATFORM_TOOLS_URL" "$archive"
  local unpack_dir="$BUILD_DIR/platform-tools-unpack"
  rm -rf "$unpack_dir" "$tools_dir"
  mkdir -p "$unpack_dir"
  (cd "$unpack_dir" && jar xf "$archive")
  mv "$unpack_dir/platform-tools" "$tools_dir"
  chmod +x "$tools_dir/"*
  log "Installed Android platform-tools cache"
}

install_android_platform_cache() {
  local platform_dir="$ANDROID_HOME_DIR/platforms/android-$ANDROID_API"
  if [ -f "$platform_dir/android.jar" ]; then
    log "Using cached Android platform android-$ANDROID_API"
    return
  fi

  local archive="$DOWNLOAD_DIR/platform-$ANDROID_API.zip"
  download "$ANDROID_PLATFORM_URL" "$archive"
  local unpack_dir="$BUILD_DIR/platform-unpack"
  rm -rf "$unpack_dir" "$platform_dir"
  mkdir -p "$unpack_dir" "$ANDROID_HOME_DIR/platforms"
  (cd "$unpack_dir" && jar xf "$archive")
  mv "$unpack_dir/android-$ANDROID_API" "$platform_dir"
  log "Installed Android platform cache: android-$ANDROID_API"
}

detect_android_api() {
  local gradle_file="$ROOT_DIR/apps/tauri/src-tauri/gen/android/app/build.gradle.kts"
  if [ -f "$gradle_file" ]; then
    local detected
    detected="$(sed -n 's/^[[:space:]]*compileSdk[[:space:]]*=[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$gradle_file" | head -n 1)"
    if [ -n "$detected" ] && [ "$detected" != "$ANDROID_API" ]; then
      log "Detected Android compileSdk $detected from generated Tauri project"
      ANDROID_API="$detected"
      if [ -z "${AURALUX_ANDROID_PLATFORM_URL:-}" ]; then
        ANDROID_PLATFORM_URL="https://dl.google.com/android/repository/platform-${ANDROID_API}_r01.zip"
      fi
    fi
  fi
}

detect_gradle_version() {
  local wrapper_file="$ROOT_DIR/apps/tauri/src-tauri/gen/android/gradle/wrapper/gradle-wrapper.properties"
  if [ ! -f "$wrapper_file" ]; then
    return
  fi

  local detected
  detected="$(sed -n 's/^distributionUrl=.*gradle-\([0-9][0-9.]*\)-bin\.zip.*/\1/p' "$wrapper_file" | head -n 1)"
  if [ -n "$detected" ] && [ "$detected" != "$GRADLE_VERSION" ]; then
    log "Detected Gradle $detected from generated Tauri project"
    GRADLE_VERSION="$detected"
    if [ -z "${AURALUX_GRADLE_URLS:-}" ] && [ -z "${AURALUX_GRADLE_URL:-}" ]; then
      GRADLE_URLS="https://mirrors.cloud.tencent.com/gradle/gradle-${GRADLE_VERSION}-bin.zip https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip"
    fi
  fi
}

install_gradle_distribution_cache() {
  local archive="$DOWNLOAD_DIR/gradle-${GRADLE_VERSION}-bin.zip"
  # shellcheck disable=SC2086
  download_first "$archive" $GRADLE_URLS
}

write_gradle_cache_config() {
  mkdir -p "$GRADLE_USER_HOME_DIR/init.d"
  cat > "$GRADLE_USER_HOME_DIR/gradle.properties" <<EOF
org.gradle.jvmargs=-Xmx3072m -Dfile.encoding=UTF-8
org.gradle.daemon=false
org.gradle.caching=true
systemProp.http.proxyHost=127.0.0.1
systemProp.http.proxyPort=19000
systemProp.http.nonProxyHosts=$GRADLE_NON_PROXY_HOSTS
systemProp.http.connectionTimeout=15000
systemProp.http.socketTimeout=15000
systemProp.https.proxyHost=127.0.0.1
systemProp.https.proxyPort=19000
systemProp.https.nonProxyHosts=$GRADLE_NON_PROXY_HOSTS
systemProp.https.connectionTimeout=15000
systemProp.https.socketTimeout=15000
android.useAndroidX=true
android.javaCompile.suppressSourceTargetDeprecationWarning=true
kotlin.code.style=official
EOF

  local mirror_array
  mirror_array="$(printf '%s\n' "$GRADLE_MAVEN_MIRRORS" | tr ' ' '\n' | sed '/^$/d; s#^#  "#; s#$#",#')"
  local official_repositories
  official_repositories=""
  if [ "$GRADLE_OFFICIAL_REPOSITORIES" = "1" ]; then
    official_repositories='
        google()
        mavenCentral()
        gradlePluginPortal()'
  fi
  cat > "$GRADLE_USER_HOME_DIR/init.d/auralux-repositories.gradle" <<EOF
def auraluxMirrorUrls = [
$mirror_array
]

def addAuraluxMirrors = { repos ->
  auraluxMirrorUrls.eachWithIndex { repoUrl, index ->
    repos.maven {
      name = "AuraluxMirror\${index + 1}"
      url = repoUrl
    }
  }
}

settingsEvaluated { settings ->
  try {
    settings.pluginManagement {
      repositories {
        clear()
        addAuraluxMirrors(delegate)
$official_repositories
      }
    }
  } catch (Throwable ignored) {
  }

  try {
    settings.dependencyResolutionManagement {
      repositories {
        clear()
        addAuraluxMirrors(delegate)
$official_repositories
      }
    }
  } catch (Throwable ignored) {
  }
}

gradle.beforeProject { project ->
  project.buildscript.repositories {
    clear()
    addAuraluxMirrors(delegate)
$official_repositories
  }
  project.repositories {
    clear()
    addAuraluxMirrors(delegate)
$official_repositories
  }
}
EOF

  log "Gradle cache/config home: $GRADLE_USER_HOME_DIR"
  if [ "$GRADLE_OFFICIAL_REPOSITORIES" = "1" ]; then
    log "Gradle mirror hosts bypass proxy; official repositories remain proxy-backed fallbacks"
  else
    log "Gradle uses mirror-only repositories; mirror hosts bypass proxy"
  fi
}

configure_gradle_wrapper_cache() {
  local wrapper_file="$ROOT_DIR/apps/tauri/src-tauri/gen/android/gradle/wrapper/gradle-wrapper.properties"
  local archive="$DOWNLOAD_DIR/gradle-${GRADLE_VERSION}-bin.zip"
  if [ ! -f "$wrapper_file" ]; then
    return
  fi

  local escaped_archive
  escaped_archive="${archive//\\/\\\\}"
  escaped_archive="${escaped_archive//:/\\:}"
  escaped_archive="${escaped_archive// /%20}"
  sed -i "s#^distributionUrl=.*#distributionUrl=file\\://${escaped_archive}#" "$wrapper_file"
  log "Gradle wrapper will use cached distribution: $archive"
}

stop_gradle_daemons() {
  local gradlew="$ROOT_DIR/apps/tauri/src-tauri/gen/android/gradlew"
  if [ ! -f "$gradlew" ]; then
    return
  fi

  log "Stopping stale Gradle daemons for this project cache"
  (
    cd "$ROOT_DIR/apps/tauri/src-tauri/gen/android"
    JAVA_HOME="$JDK_DIR" GRADLE_USER_HOME="$GRADLE_USER_HOME_DIR" PATH="$JDK_DIR/bin:$PATH" ./gradlew --stop >/dev/null 2>&1 || true
  )
}

install_android_build_tools_cache() {
  local build_tools_dir="$ANDROID_HOME_DIR/build-tools/$ANDROID_BUILD_TOOLS"
  if [ -x "$build_tools_dir/aapt2" ]; then
    log "Using cached Android build-tools $ANDROID_BUILD_TOOLS"
    return
  fi

  local archive="$DOWNLOAD_DIR/build-tools_r35_linux.zip"
  download "$ANDROID_BUILD_TOOLS_URL" "$archive"
  local unpack_dir="$BUILD_DIR/build-tools-unpack"
  rm -rf "$unpack_dir" "$build_tools_dir"
  mkdir -p "$unpack_dir" "$ANDROID_HOME_DIR/build-tools"
  (cd "$unpack_dir" && jar xf "$archive")
  mv "$unpack_dir/android-15" "$build_tools_dir"
  chmod +x "$build_tools_dir/"*
  log "Installed Android build-tools cache: $ANDROID_BUILD_TOOLS"
}

install_android_cmake_cache() {
  local cmake_dir="$ANDROID_HOME_DIR/cmake/$ANDROID_CMAKE_VERSION"
  if [ -x "$cmake_dir/bin/cmake" ]; then
    log "Using cached Android CMake: $("$cmake_dir/bin/cmake" --version | head -n 1)"
    return
  fi

  local archive="$DOWNLOAD_DIR/cmake-$ANDROID_CMAKE_VERSION-linux.zip"
  download "$ANDROID_CMAKE_URL" "$archive"
  rm -rf "$cmake_dir"
  mkdir -p "$cmake_dir"
  (cd "$cmake_dir" && jar xf "$archive")
  cat > "$cmake_dir/source.properties" <<EOF
Pkg.Desc=CMake $ANDROID_CMAKE_VERSION
Pkg.Revision=$ANDROID_CMAKE_VERSION
Pkg.Path=cmake;$ANDROID_CMAKE_VERSION
EOF
  chmod +x "$cmake_dir/bin/"*
  log "Installed Android CMake cache: $("$cmake_dir/bin/cmake" --version | head -n 1)"
}

install_android_ndk_cache() {
  local ndk_dir="$ANDROID_HOME_DIR/ndk/$ANDROID_NDK_VERSION"
  if [ -x "$ndk_dir/ndk-build" ]; then
    log "Using cached Android NDK: $ANDROID_NDK_VERSION"
    repair_android_ndk_cache "$ndk_dir"
    return
  fi

  local archive="$DOWNLOAD_DIR/android-ndk-r27c-linux.zip"
  download "$ANDROID_NDK_URL" "$archive"
  local unpack_dir="$BUILD_DIR/ndk-unpack"
  rm -rf "$unpack_dir" "$ndk_dir"
  mkdir -p "$unpack_dir" "$ANDROID_HOME_DIR/ndk"
  (cd "$unpack_dir" && jar xf "$archive")
  mv "$unpack_dir/android-ndk-r27c" "$ndk_dir"
  repair_android_ndk_cache "$ndk_dir"
  log "Installed Android NDK cache: $ANDROID_NDK_VERSION"
}

repair_android_ndk_cache() {
  local ndk_dir="$1"
  chmod +x "$ndk_dir"/ndk-* \
    "$ndk_dir/prebuilt/linux-x86_64/bin/"* \
    "$ndk_dir/toolchains/llvm/prebuilt/linux-x86_64/bin/"* \
    2>/dev/null || true

  local llvm_bin="$ndk_dir/toolchains/llvm/prebuilt/linux-x86_64/bin"
  if [ ! -d "$llvm_bin" ]; then
    return
  fi

  local file target
  find "$llvm_bin" -maxdepth 1 -type f -size -128c -print0 | while IFS= read -r -d '' file; do
    target="$(tr -d '\r\n' < "$file" 2>/dev/null || true)"
    case "$target" in
      ""|*/*|*".. "*|*" "*)
        continue
        ;;
    esac
    if [ -e "$llvm_bin/$target" ]; then
      rm -f "$file"
      ln -s "$target" "$file"
    fi
  done
}

install_rust_android_targets() {
  local targets=(
    aarch64-linux-android
    armv7-linux-androideabi
    i686-linux-android
    x86_64-linux-android
  )

  local target missing=()
  for target in "${targets[@]}"; do
    if ! rustup target list --installed | grep -qx "$target"; then
      missing+=("$target")
    fi
  done

  if [ "${#missing[@]}" -eq 0 ]; then
    log "Rust Android targets already installed"
    return
  fi

  log "Installing Rust Android targets: ${missing[*]}"
  local rustup_cmd=(rustup target add "${missing[@]}")
  if is_mirror_url "$RUSTUP_DIST_SERVER"; then
    rustup_cmd=(env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy "${rustup_cmd[@]}")
  fi

  if ! "${rustup_cmd[@]}"; then
    log "Primary rustup mirror failed, retrying official static.rust-lang.org through proxy"
    RUSTUP_DIST_SERVER="https://static.rust-lang.org" \
      RUSTUP_UPDATE_ROOT="https://static.rust-lang.org/rustup" \
      rustup target add "${missing[@]}"
  fi
}

build_frontend() {
  if [ ! -d node_modules ]; then
    log "Installing npm dependencies"
    npm install
  fi
  log "Building shared GUI"
  npm --workspace apps/gui run build
}

init_android_project() {
  if [ -f apps/tauri/src-tauri/gen/android/gradlew ]; then
    log "Android project already initialized"
    return
  fi

  log "Initializing Tauri Android project"
  npm --workspace apps/tauri exec tauri -- android init --ci
}

build_apk() {
  export JAVA_HOME="$JDK_DIR"
  export ANDROID_HOME="$ANDROID_HOME_DIR"
  export ANDROID_SDK_ROOT="$ANDROID_HOME_DIR"
  export NDK_HOME="$ANDROID_HOME/ndk/$ANDROID_NDK_VERSION"
  export ANDROID_NDK_HOME="$NDK_HOME"
  export GRADLE_USER_HOME="$GRADLE_USER_HOME_DIR"
  export PATH="$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

  log "Building Android APK for ABI: $ANDROID_ABI"
  npm --workspace apps/tauri exec tauri -- android build --apk --target "$ANDROID_ABI" --ci --split-per-abi
}

stage_artifacts() {
  local stage_dir="$ROOT_DIR/release/auralux-android"
  rm -rf "$stage_dir"
  mkdir -p "$stage_dir"

  find apps/tauri/src-tauri/gen/android -type f \( -name '*.apk' -o -name '*.aab' \) -print -exec cp {} "$stage_dir/" \;
  if ! find "$stage_dir" -type f -name '*.apk' | grep -q .; then
    printf 'No APK was produced under apps/tauri/src-tauri/gen/android.\n' >&2
    exit 1
  fi

  (
    cd "$stage_dir"
    sha256sum *.apk > SHA256SUMS
  )

  log "Android APK artifacts staged in release/auralux-android"
  log "Download cache preserved in $DOWNLOAD_DIR"
  log "Gradle cache preserved in $GRADLE_USER_HOME_DIR"
  log "npm cache preserved in $NPM_CACHE_DIR"
}

main() {
  case "${1:-}" in
    -h|--help|help)
      usage
      return 0
      ;;
    "")
      ;;
    *)
      printf 'Unknown argument: %s\n\n' "$1" >&2
      usage >&2
      return 2
      ;;
  esac

  require_cmd npm
  require_cmd rustup
  require_cmd cargo
  require_cmd wget
  require_cmd tar
  require_cmd sha256sum

  configure_proxy
  install_jdk
  install_android_cmdline_tools
  init_android_project
  detect_android_api
  detect_gradle_version
  install_android_packages
  install_gradle_distribution_cache
  write_gradle_cache_config
  configure_gradle_wrapper_cache
  stop_gradle_daemons
  install_rust_android_targets
  build_frontend
  build_apk
  stage_artifacts
}

main "$@"

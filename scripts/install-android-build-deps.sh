#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
PACKAGE_JSON_PATH="${REPO_ROOT}/package.json"

ANDROID_SDK_ROOT_DEFAULT="${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}"
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT_DEFAULT}"
ANDROID_HOME="${ANDROID_HOME:-$ANDROID_SDK_ROOT}"

CMDLINE_TOOLS_URL_DEFAULT="https://dl.google.com/android/repository/commandlinetools-linux-14742923_latest.zip"
CMDLINE_TOOLS_URL="${ANDROID_CMDLINE_TOOLS_URL:-$CMDLINE_TOOLS_URL_DEFAULT}"

PLATFORM_PACKAGE_FALLBACK="platforms;android-35"
BUILD_TOOLS_PACKAGE_FALLBACK="build-tools;35.0.0"
NDK_PACKAGE_FALLBACK="ndk;29.0.14206865"

APT_PACKAGES=(
  openjdk-17-jdk
  nodejs
  npm
  curl
  wget
  unzip
  git
  build-essential
  pkg-config
  libssl-dev
  ca-certificates
)

RUST_ANDROID_TARGETS=(
  aarch64-linux-android
  armv7-linux-androideabi
  i686-linux-android
  x86_64-linux-android
)

log() {
  printf '[android-deps] %s\n' "$*"
}

warn() {
  printf '[android-deps] warning: %s\n' "$*" >&2
}

fail() {
  printf '[android-deps] error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<EOF
Usage:
  scripts/install-android-build-deps.sh

Host support:
  - Linux only
  - Ubuntu/Debian-family systems only

This script installs:
  - OpenJDK 17 and required native packages
  - Android SDK command-line tools into: ${ANDROID_SDK_ROOT}
  - Android platform-tools, one recent platform/build-tools, cmdline-tools, and a recent NDK
  - Rust Android targets used by Tauri mobile builds

Optional environment variables:
  ANDROID_SDK_ROOT
  ANDROID_HOME
  ANDROID_CMDLINE_TOOLS_URL
EOF
}

require_command() {
  local command_name="$1"
  command -v "$command_name" >/dev/null 2>&1 || fail "missing required command: $command_name"
}

ensure_linux_host() {
  local os_name=""
  os_name="$(uname -s 2>/dev/null || true)"
  [[ "$os_name" == "Linux" ]] || fail "this script only supports Linux hosts (detected: ${os_name:-unknown})"
}

resolve_symlink_path() {
  local target_path="$1"
  local link_target=""

  [[ -e "$target_path" ]] || return 1

  while [[ -L "$target_path" ]]; do
    link_target="$(readlink "$target_path")" || return 1
    if [[ "$link_target" = /* ]]; then
      target_path="$link_target"
      continue
    fi

    target_path="$(cd -- "$(dirname -- "$target_path")" && cd -- "$(dirname -- "$link_target")" && pwd -P)/$(basename -- "$link_target")"
  done

  printf '%s\n' "$(cd -- "$(dirname -- "$target_path")" && pwd -P)/$(basename -- "$target_path")"
}

ensure_debian_family() {
  [[ -r /etc/os-release ]] || fail "cannot read /etc/os-release"

  # shellcheck disable=SC1091
  . /etc/os-release

  if [[ "${ID:-}" == "debian" || "${ID:-}" == "ubuntu" || "${ID_LIKE:-}" == *"debian"* ]]; then
    return 0
  fi

  fail "this script only supports Ubuntu/Debian-family systems"
}

ensure_apt_tooling() {
  require_command apt-get
  require_command dpkg
}

setup_sudo() {
  if [[ "${EUID}" -eq 0 ]]; then
    APT_PREFIX=()
    return 0
  fi

  require_command sudo
  APT_PREFIX=(sudo)
}

package_is_installed() {
  dpkg -s "$1" >/dev/null 2>&1
}

install_apt_packages() {
  local missing_packages=()
  local package_name=""

  for package_name in "${APT_PACKAGES[@]}"; do
    if ! package_is_installed "$package_name"; then
      missing_packages+=("$package_name")
    fi
  done

  if (( ${#missing_packages[@]} == 0 )); then
    log "APT packages already installed"
    return 0
  fi

  log "Installing APT packages: ${missing_packages[*]}"
  "${APT_PREFIX[@]}" apt-get update
  DEBIAN_FRONTEND=noninteractive "${APT_PREFIX[@]}" apt-get install -y "${missing_packages[@]}"
}

ensure_rustup() {
  if command -v rustup >/dev/null 2>&1 && command -v cargo >/dev/null 2>&1; then
    return 0
  fi

  log "Installing rustup with the minimal profile"

  if command -v curl >/dev/null 2>&1; then
    curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- https://sh.rustup.rs | sh -s -- -y --profile minimal
  else
    fail "neither curl nor wget is available to install rustup"
  fi
}

load_cargo_env() {
  if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    . "$HOME/.cargo/env"
  fi

  require_command rustup
  require_command cargo
}

detect_pnpm_package_spec() {
  local package_manager=""

  if [[ -f "$PACKAGE_JSON_PATH" ]]; then
    package_manager="$(
      sed -nE 's/.*"packageManager"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' "$PACKAGE_JSON_PATH" \
        | head -n 1
    )"
  fi

  if [[ "$package_manager" == pnpm@* ]]; then
    printf '%s\n' "$package_manager"
    return 0
  fi

  printf 'pnpm\n'
}

ensure_node_toolchain() {
  local pnpm_package_spec=""

  require_command node
  pnpm_package_spec="$(detect_pnpm_package_spec)"

  if command -v corepack >/dev/null 2>&1; then
    log "Activating ${pnpm_package_spec} with corepack"
    corepack enable >/dev/null 2>&1 || true
    corepack prepare "$pnpm_package_spec" --activate >/dev/null 2>&1 || true
  fi

  if command -v pnpm >/dev/null 2>&1; then
    return 0
  fi

  require_command npm
  log "Installing ${pnpm_package_spec} globally"
  "${APT_PREFIX[@]}" npm install -g "$pnpm_package_spec"
  require_command pnpm
}

ensure_node_version_supported() {
  local node_version_raw=""
  local node_major=""

  node_version_raw="$(node --version 2>/dev/null || true)"
  node_major="${node_version_raw#v}"
  node_major="${node_major%%.*}"

  [[ -n "$node_major" ]] || fail "unable to parse Node.js version: ${node_version_raw:-unknown}"
  [[ "$node_major" =~ ^[0-9]+$ ]] || fail "unable to parse Node.js major version: ${node_version_raw}"
  (( node_major >= 18 )) || fail "Node.js ${node_version_raw} is too old; install Node.js 18+ (recommend 20 LTS)"
}

find_sdkmanager() {
  local candidate=""

  for candidate in \
    "$ANDROID_SDK_ROOT/cmdline-tools/latest/bin/sdkmanager" \
    "$ANDROID_SDK_ROOT/cmdline-tools/bin/sdkmanager"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  while IFS= read -r candidate; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done < <(find "$ANDROID_SDK_ROOT/cmdline-tools" -type f -name sdkmanager 2>/dev/null | sort)

  return 1
}

download_with_available_tool() {
  local url="$1"
  local output_path="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --progress-bar "$url" -o "$output_path"
    return 0
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -O "$output_path" "$url"
    return 0
  fi

  fail "curl or wget is required to download Android command-line tools"
}

bootstrap_cmdline_tools() {
  local sdkmanager_path=""

  mkdir -p "$ANDROID_SDK_ROOT"

  if sdkmanager_path="$(find_sdkmanager)"; then
    log "Android command-line tools already present at ${sdkmanager_path}"
    return 0
  fi

  local temp_dir=""
  local zip_path=""
  temp_dir="$(mktemp -d)"
  zip_path="${temp_dir}/cmdline-tools.zip"

  log "Downloading Android command-line tools from ${CMDLINE_TOOLS_URL}"
  download_with_available_tool "$CMDLINE_TOOLS_URL" "$zip_path"

  mkdir -p "$ANDROID_SDK_ROOT/cmdline-tools"
  unzip -q -o "$zip_path" -d "$temp_dir/unpacked"

  rm -rf "$ANDROID_SDK_ROOT/cmdline-tools/latest"
  mkdir -p "$ANDROID_SDK_ROOT/cmdline-tools/latest"

  if [[ -d "$temp_dir/unpacked/cmdline-tools" ]]; then
    mv "$temp_dir/unpacked/cmdline-tools/"* "$ANDROID_SDK_ROOT/cmdline-tools/latest/"
  else
    fail "unexpected command-line tools archive layout"
  fi

  rm -rf "$temp_dir"
}

accept_android_licenses() {
  local sdkmanager_path="$1"
  local sdkmanager_status=0

  log "Accepting Android SDK licenses"
  set +o pipefail
  yes | "$sdkmanager_path" --sdk_root="$ANDROID_SDK_ROOT" --licenses >/dev/null
  sdkmanager_status=$?
  set -o pipefail

  if [[ "$sdkmanager_status" -ne 0 ]]; then
    fail "sdkmanager --licenses failed with exit code ${sdkmanager_status}"
  fi
}

latest_matching_package() {
  local sdk_list="$1"
  local regex="$2"
  local fallback="$3"
  local package_id=""

  package_id="$(
    printf '%s\n' "$sdk_list" \
      | awk -F'|' '{gsub(/^[[:space:]]+|[[:space:]]+$/, "", $1); print $1}' \
      | grep -E "$regex" \
      | sort -V \
      | tail -n 1 || true
  )"

  if [[ -n "$package_id" ]]; then
    printf '%s\n' "$package_id"
    return 0
  fi

  printf '%s\n' "$fallback"
}

install_android_packages() {
  local sdkmanager_path="$1"
  local sdk_list=""
  local platform_package=""
  local build_tools_package=""
  local ndk_package=""

  sdk_list="$("$sdkmanager_path" --sdk_root="$ANDROID_SDK_ROOT" --list 2>/dev/null | tr -d '\r')"

  platform_package="$(latest_matching_package "$sdk_list" '^platforms;android-[0-9]+$' "$PLATFORM_PACKAGE_FALLBACK")"
  build_tools_package="$(latest_matching_package "$sdk_list" '^build-tools;[0-9.]+$' "$BUILD_TOOLS_PACKAGE_FALLBACK")"
  ndk_package="$(latest_matching_package "$sdk_list" '^ndk;[0-9.]+$' "$NDK_PACKAGE_FALLBACK")"

  log "Installing Android SDK packages:"
  log "  cmdline-tools;latest"
  log "  platform-tools"
  log "  ${platform_package}"
  log "  ${build_tools_package}"
  log "  ${ndk_package}"

  "$sdkmanager_path" --sdk_root="$ANDROID_SDK_ROOT" --install \
    "cmdline-tools;latest" \
    "platform-tools" \
    "$platform_package" \
    "$build_tools_package" \
    "$ndk_package"
}

install_rust_android_targets() {
  local target=""

  for target in "${RUST_ANDROID_TARGETS[@]}"; do
    if rustup target list --installed | grep -Fxq "$target"; then
      log "Rust target already installed: ${target}"
      continue
    fi

    log "Installing Rust target: ${target}"
    rustup target add "$target"
  done
}

detect_java_home() {
  local java_bin=""

  java_bin="$(command -v javac || command -v java || true)"
  if [[ -z "$java_bin" ]]; then
    return 1
  fi

  java_bin="$(resolve_symlink_path "$java_bin")"
  dirname "$(dirname "$java_bin")"
}

print_env_hints() {
  local java_home=""

  java_home="$(detect_java_home || true)"

  cat <<EOF

Environment export hints:
  export ANDROID_HOME="${ANDROID_HOME}"
  export ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT}"
EOF

  if [[ -n "$java_home" ]]; then
    cat <<EOF
  export JAVA_HOME="${java_home}"
EOF
  fi

  cat <<EOF
  export PATH="${ANDROID_SDK_ROOT}/platform-tools:${ANDROID_SDK_ROOT}/cmdline-tools/latest/bin:\$PATH"
EOF

  if [[ -f "$HOME/.cargo/env" ]]; then
    cat <<EOF
  source "$HOME/.cargo/env"
EOF
  fi

  cat <<EOF
  pnpm install --frozen-lockfile
  pnpm android:apk
EOF
}

main() {
  local sdkmanager_path=""

  for arg in "$@"; do
    case "$arg" in
      -h|--help)
        usage
        exit 0
        ;;
    esac
  done

  ensure_linux_host
  ensure_debian_family
  ensure_apt_tooling
  setup_sudo
  install_apt_packages
  ensure_rustup
  load_cargo_env
  ensure_node_toolchain
  ensure_node_version_supported

  export ANDROID_SDK_ROOT
  export ANDROID_HOME

  bootstrap_cmdline_tools
  sdkmanager_path="$(find_sdkmanager)" || fail "sdkmanager not found after installing command-line tools"

  accept_android_licenses "$sdkmanager_path"
  install_android_packages "$sdkmanager_path"
  accept_android_licenses "$sdkmanager_path"
  install_rust_android_targets

  log "Android build dependencies are ready."
  print_env_hints
}

main "$@"

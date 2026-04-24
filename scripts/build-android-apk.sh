#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
ANDROID_GEN_DIR="${REPO_ROOT}/src-tauri/gen/android"
ANDROID_OUTPUT_DIR="${ANDROID_GEN_DIR}/app/build/outputs"

log() {
  printf '[android-build] %s\n' "$*"
}

warn() {
  printf '[android-build] warning: %s\n' "$*" >&2
}

fail() {
  printf '[android-build] error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage:
  scripts/build-android-apk.sh [tauri android build options]

Host support:
  - Linux only
  - intended to be paired with scripts/install-android-build-deps.sh on Debian/Ubuntu

Default behavior:
  - builds an Android APK in CI mode
  - defaults to --target aarch64 when no target is provided
  - auto-runs `pnpm tauri android init --ci --skip-targets-install` on first use
  - checks java, javac, pnpm, cargo, rustup, Android SDK, and sdkmanager first

Examples:
  scripts/build-android-apk.sh
  scripts/build-android-apk.sh --debug
  scripts/build-android-apk.sh --target aarch64 x86_64 --split-per-abi
  scripts/build-android-apk.sh --aab
EOF
}

require_command() {
  local command_name="$1"
  command -v "${command_name}" >/dev/null 2>&1 || fail "missing required command: ${command_name}"
}

ensure_linux_host() {
  local os_name=""
  os_name="$(uname -s 2>/dev/null || true)"
  [[ "${os_name}" == "Linux" ]] || fail "this script only supports Linux hosts (detected: ${os_name:-unknown})"
}

ensure_node_version_supported() {
  local node_version_raw=""
  local node_major=""

  node_version_raw="$(node --version 2>/dev/null || true)"
  node_major="${node_version_raw#v}"
  node_major="${node_major%%.*}"

  [[ -n "${node_major}" ]] || fail "unable to parse Node.js version: ${node_version_raw:-unknown}"
  [[ "${node_major}" =~ ^[0-9]+$ ]] || fail "unable to parse Node.js major version: ${node_version_raw}"
  (( node_major >= 18 )) || fail "Node.js ${node_version_raw} is too old; install Node.js 18+"
}

resolve_symlink_path() {
  local target_path="$1"
  local link_target=""

  [[ -e "${target_path}" ]] || return 1

  while [[ -L "${target_path}" ]]; do
    link_target="$(readlink "${target_path}")" || return 1
    if [[ "${link_target}" = /* ]]; then
      target_path="${link_target}"
      continue
    fi

    target_path="$(cd -- "$(dirname -- "${target_path}")" && cd -- "$(dirname -- "${link_target}")" && pwd -P)/$(basename -- "${link_target}")"
  done

  printf '%s\n' "$(cd -- "$(dirname -- "${target_path}")" && pwd -P)/$(basename -- "${target_path}")"
}

resolve_java_home() {
  local java_candidate=""

  if [[ -n "${JAVA_HOME:-}" && -x "${JAVA_HOME}/bin/java" ]]; then
    return 0
  fi

  if command -v javac >/dev/null 2>&1; then
    java_candidate="$(resolve_symlink_path "$(command -v javac)")"
    export JAVA_HOME="$(cd -- "$(dirname -- "${java_candidate}")/.." && pwd)"
    return 0
  fi

  if command -v java >/dev/null 2>&1; then
    java_candidate="$(resolve_symlink_path "$(command -v java)")"
    export JAVA_HOME="$(cd -- "$(dirname -- "${java_candidate}")/.." && pwd)"
    return 0
  fi

  return 1
}

resolve_android_sdk_root() {
  local candidate=""

  if [[ -n "${ANDROID_SDK_ROOT:-}" && -d "${ANDROID_SDK_ROOT}" ]]; then
    candidate="${ANDROID_SDK_ROOT}"
  elif [[ -n "${ANDROID_HOME:-}" && -d "${ANDROID_HOME}" ]]; then
    candidate="${ANDROID_HOME}"
  elif [[ -d "${HOME}/Android/Sdk" ]]; then
    candidate="${HOME}/Android/Sdk"
  fi

  [[ -n "${candidate}" ]] || return 1

  export ANDROID_HOME="${candidate}"
  export ANDROID_SDK_ROOT="${candidate}"
}

normalize_android_signing_env() {
  local keystore_path=""
  local keystore_password=""
  local key_alias=""
  local key_password=""

  keystore_path="${TAURI_ANDROID_KEYSTORE_PATH:-${TAURI_ANDROID_KEYSTORE:-${ANDROID_KEYSTORE_PATH:-${ANDROID_KEYSTORE:-}}}}"
  keystore_password="${TAURI_ANDROID_KEYSTORE_PASSWORD:-${ANDROID_KEYSTORE_PASSWORD:-}}"
  key_alias="${TAURI_ANDROID_KEY_ALIAS:-${ANDROID_KEY_ALIAS:-}}"
  key_password="${TAURI_ANDROID_KEY_PASSWORD:-${ANDROID_KEY_PASSWORD:-}}"

  if [[ -n "${keystore_path}" ]]; then
    export TAURI_ANDROID_KEYSTORE_PATH="${TAURI_ANDROID_KEYSTORE_PATH:-${keystore_path}}"
    export TAURI_ANDROID_KEYSTORE="${TAURI_ANDROID_KEYSTORE:-${keystore_path}}"
    export ANDROID_KEYSTORE_PATH="${ANDROID_KEYSTORE_PATH:-${keystore_path}}"
  fi

  if [[ -n "${keystore_password}" ]]; then
    export TAURI_ANDROID_KEYSTORE_PASSWORD="${TAURI_ANDROID_KEYSTORE_PASSWORD:-${keystore_password}}"
    export ANDROID_KEYSTORE_PASSWORD="${ANDROID_KEYSTORE_PASSWORD:-${keystore_password}}"
  fi

  if [[ -n "${key_alias}" ]]; then
    export TAURI_ANDROID_KEY_ALIAS="${TAURI_ANDROID_KEY_ALIAS:-${key_alias}}"
    export ANDROID_KEY_ALIAS="${ANDROID_KEY_ALIAS:-${key_alias}}"
  fi

  if [[ -n "${key_password}" ]]; then
    export TAURI_ANDROID_KEY_PASSWORD="${TAURI_ANDROID_KEY_PASSWORD:-${key_password}}"
    export ANDROID_KEY_PASSWORD="${ANDROID_KEY_PASSWORD:-${key_password}}"
  fi
}

find_sdk_tool() {
  local tool_name="$1"
  local candidate=""

  if command -v "${tool_name}" >/dev/null 2>&1; then
    command -v "${tool_name}"
    return 0
  fi

  if [[ -n "${ANDROID_SDK_ROOT:-}" ]]; then
    for candidate in \
      "${ANDROID_SDK_ROOT}/platform-tools/${tool_name}" \
      "${ANDROID_SDK_ROOT}/cmdline-tools/latest/bin/${tool_name}" \
      "${ANDROID_SDK_ROOT}/cmdline-tools/bin/${tool_name}"; do
      if [[ -x "${candidate}" ]]; then
        printf '%s\n' "${candidate}"
        return 0
      fi
    done

    while IFS= read -r candidate; do
      if [[ -x "${candidate}" ]]; then
        printf '%s\n' "${candidate}"
        return 0
      fi
    done < <(find "${ANDROID_SDK_ROOT}/cmdline-tools" -type f -name "${tool_name}" 2>/dev/null | sort)
  fi

  return 1
}

rust_triple_for_target() {
  case "$1" in
    aarch64) printf 'aarch64-linux-android\n' ;;
    armv7) printf 'armv7-linux-androideabi\n' ;;
    i686) printf 'i686-linux-android\n' ;;
    x86_64) printf 'x86_64-linux-android\n' ;;
    *)
      return 1
      ;;
  esac
}

extract_requested_targets() {
  local args=("$@")
  local result=()
  local index=0
  local raw_target=""
  local inline_targets=()
  local inline_target=""

  while (( index < ${#args[@]} )); do
    case "${args[index]}" in
      --target|-t)
        (( index += 1 ))
        while (( index < ${#args[@]} )); do
          case "${args[index]}" in
            -*)
              (( index -= 1 ))
              break
              ;;
            *)
              raw_target="${args[index]}"
              IFS=',' read -r -a inline_targets <<< "${raw_target}"
              for inline_target in "${inline_targets[@]}"; do
                [[ -n "${inline_target}" ]] || continue
                result+=("${inline_target}")
              done
              ;;
          esac
          (( index += 1 ))
        done
        ;;
      --target=*|-t=*)
        raw_target="${args[index]#*=}"
        IFS=',' read -r -a inline_targets <<< "${raw_target}"
        for inline_target in "${inline_targets[@]}"; do
          [[ -n "${inline_target}" ]] || continue
          result+=("${inline_target}")
        done
        ;;
    esac
    (( index += 1 ))
  done

  printf '%s\n' "${result[@]}"
}

ensure_js_dependencies() {
  if [[ -d "${REPO_ROOT}/node_modules" && -f "${REPO_ROOT}/node_modules/.modules.yaml" ]]; then
    return 0
  fi

  if [[ -f "${REPO_ROOT}/pnpm-lock.yaml" ]]; then
    log "Installing JavaScript dependencies with pnpm install --frozen-lockfile"
    pnpm install --frozen-lockfile
    return 0
  fi

  log "Installing JavaScript dependencies with pnpm install"
  pnpm install
}

ensure_rust_target() {
  local rust_triple="$1"

  if rustup target list --installed | grep -Fxq "${rust_triple}"; then
    return 0
  fi

  log "Installing Rust target ${rust_triple}"
  rustup target add "${rust_triple}"
}

verify_release_signing_configuration() {
  local using_aab="$1"
  local is_release="$2"
  local has_keystore_properties=0
  local keystore_path=""
  local path_candidates=()
  local candidate=""
  local has_complete_env=0

  if [[ "${is_release}" -eq 0 ]]; then
    return 0
  fi

  if [[ -f "${ANDROID_GEN_DIR}/keystore.properties" ]]; then
    has_keystore_properties=1
  fi

  path_candidates=("${TAURI_ANDROID_KEYSTORE_PATH:-}" "${TAURI_ANDROID_KEYSTORE:-}")
  for candidate in "${path_candidates[@]}"; do
    [[ -n "${candidate}" ]] || continue
    keystore_path="${candidate}"
    break
  done

  if [[ -n "${keystore_path}" ]]; then
    [[ -f "${keystore_path}" ]] || fail "Android keystore file not found: ${keystore_path}"
  fi

  if [[ -n "${keystore_path}" && -n "${TAURI_ANDROID_KEYSTORE_PASSWORD:-}" && -n "${TAURI_ANDROID_KEY_ALIAS:-}" && -n "${TAURI_ANDROID_KEY_PASSWORD:-}" ]]; then
    has_complete_env=1
  fi

  if (( has_keystore_properties == 1 || has_complete_env == 1 )); then
    return 0
  fi

  if [[ "${using_aab}" -eq 1 ]]; then
    fail "release AAB requires signing config; provide ${ANDROID_GEN_DIR}/keystore.properties or TAURI_ANDROID_KEYSTORE_PATH/TAURI_ANDROID_KEYSTORE + TAURI_ANDROID_KEYSTORE_PASSWORD + TAURI_ANDROID_KEY_ALIAS + TAURI_ANDROID_KEY_PASSWORD (ANDROID_KEYSTORE_PATH + ANDROID_KEYSTORE_PASSWORD + ANDROID_KEY_ALIAS + ANDROID_KEY_PASSWORD are also accepted)"
  fi

  warn "release build has no explicit signing config; artifact may be unsuitable for distribution"
  warn "set ${ANDROID_GEN_DIR}/keystore.properties or TAURI_ANDROID_KEYSTORE_PATH/TAURI_ANDROID_KEYSTORE + TAURI_ANDROID_KEYSTORE_PASSWORD + TAURI_ANDROID_KEY_ALIAS + TAURI_ANDROID_KEY_PASSWORD"
  warn "ANDROID_KEYSTORE_PATH + ANDROID_KEYSTORE_PASSWORD + ANDROID_KEY_ALIAS + ANDROID_KEY_PASSWORD aliases are also supported"
}

print_artifacts() {
  if [[ ! -d "${ANDROID_OUTPUT_DIR}" ]]; then
    warn "Android output directory not found yet: ${ANDROID_OUTPUT_DIR}"
    return 0
  fi

  local artifact_count=0
  local artifact_path=""
  local artifacts=()

  while IFS= read -r artifact_path; do
    [[ -n "${artifact_path}" ]] || continue
    artifacts+=("${artifact_path}")
  done < <(find "${ANDROID_OUTPUT_DIR}" -type f \( -name '*.apk' -o -name '*.aab' \) | sort)

  artifact_count="${#artifacts[@]}"
  if (( artifact_count == 0 )); then
    warn "build finished but no APK/AAB artifacts were found under ${ANDROID_OUTPUT_DIR}"
    return 0
  fi

  log "Artifacts:"
  for artifact_path in "${artifacts[@]}"; do
    printf '  - %s\n' "${artifact_path#${REPO_ROOT}/}"
  done
}

cd "${REPO_ROOT}"

RAW_USER_ARGS=("$@")
USER_ARGS=()

for arg in "${RAW_USER_ARGS[@]}"; do
  case "${arg}" in
    --)
      continue
      ;;
    -h|--help)
      usage
      exit 0
      ;;
  esac

  USER_ARGS+=("${arg}")
done

ensure_linux_host
require_command pnpm
require_command node
require_command cargo
require_command rustup
require_command java
require_command javac
ensure_node_version_supported

resolve_java_home || fail "JAVA_HOME could not be resolved automatically."

resolve_android_sdk_root || fail "Android SDK not found. Set ANDROID_SDK_ROOT or ANDROID_HOME first."
normalize_android_signing_env

SDKMANAGER_BIN="$(find_sdk_tool sdkmanager || true)"
ADB_BIN="$(find_sdk_tool adb || true)"

[[ -n "${SDKMANAGER_BIN}" ]] || fail "sdkmanager not found under ${ANDROID_SDK_ROOT}. Install Android cmdline-tools."
if [[ -z "${ADB_BIN}" ]]; then
  warn "adb not found under ${ANDROID_SDK_ROOT}; build can still work, but device install/run will not."
fi

log "Repo root: ${REPO_ROOT}"
log "Android SDK: ${ANDROID_SDK_ROOT}"
log "JAVA_HOME: ${JAVA_HOME}"
log "Java: $(java -version 2>&1 | head -n 1)"
log "Node: $(node --version)"
log "pnpm: $(pnpm --version)"
log "Cargo: $(cargo --version)"

BUILD_ARGS=(android build --ci)
TARGETS=()
TARGET_SPECIFIED=0
ARTIFACT_SPECIFIED=0
USING_AAB=0
IS_RELEASE_BUILD=1

for arg in "${USER_ARGS[@]}"; do
  case "${arg}" in
    --target|-t|--target=*|-t=*)
      TARGET_SPECIFIED=1
      ;;
    --aab)
      ARTIFACT_SPECIFIED=1
      USING_AAB=1
      ;;
    --apk)
      ARTIFACT_SPECIFIED=1
      ;;
    --debug)
      IS_RELEASE_BUILD=0
      ;;
  esac
done

if (( ! ARTIFACT_SPECIFIED )); then
  BUILD_ARGS+=(--apk)
fi

if (( ! TARGET_SPECIFIED )); then
  TARGETS=("aarch64")
  BUILD_ARGS+=(--target "${TARGETS[@]}")
else
  mapfile -t TARGETS < <(extract_requested_targets "${USER_ARGS[@]}")
  (( ${#TARGETS[@]} > 0 )) || fail "at least one Android target must follow --target or -t"
fi

BUILD_ARGS+=("${USER_ARGS[@]}")

if (( ${#TARGETS[@]} > 0 )); then
  for target_name in "${TARGETS[@]}"; do
    rust_triple="$(rust_triple_for_target "${target_name}")" || fail "unsupported Android target: ${target_name}"
    ensure_rust_target "${rust_triple}"
  done
fi

ensure_js_dependencies
if ! pnpm exec tauri --version >/dev/null 2>&1; then
  fail "Tauri CLI not found in project dependencies. Install @tauri-apps/cli and run pnpm install."
fi

verify_release_signing_configuration "${USING_AAB}" "${IS_RELEASE_BUILD}"

if [[ ! -d "${ANDROID_GEN_DIR}" ]]; then
  log "Android project not initialized yet; running tauri android init --ci --skip-targets-install"
  pnpm tauri android init --ci --skip-targets-install
fi

log "Running: pnpm tauri ${BUILD_ARGS[*]}"
pnpm tauri "${BUILD_ARGS[@]}"

log "Build finished."
print_artifacts

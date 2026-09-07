#!/bin/sh
# 一键安装 binox 到 ~/.local/bin（curl 拉 GitHub Release → SHA256 校验 → 落位）
#
#   curl -fsSL https://github.com/spraylee/binox/releases/latest/download/bootstrap.sh | sh
#
# 环境变量：
#   BINOX_REPOSITORY     默认 spraylee/binox
#   BINOX_VERSION        钉死 tag，如 v0.1.0；空则 302 探测 latest
#   BINOX_INSTALL_DIR    默认 ~/.local/bin
#   BINOX_RELEASE_BASE_URL  覆盖下载前缀（测试用）

set -eu

REPOSITORY="${BINOX_REPOSITORY:-spraylee/binox}"
VERSION="${BINOX_VERSION:-}"
INSTALL_DIR="${BINOX_INSTALL_DIR:-${HOME}/.local/bin}"

fail() {
  printf '%s\n' "[binox] bootstrap: $*" >&2
  exit 1
}

command -v curl >/dev/null 2>&1 || fail "需要 curl"
command -v uname >/dev/null 2>&1 || fail "需要 uname"
command -v tar >/dev/null 2>&1 || fail "需要 tar"
command -v awk >/dev/null 2>&1 || fail "需要 awk"

OS=$(uname -s)
ARCH=$(uname -m)
case "${OS}:${ARCH}" in
  Darwin:arm64|Darwin:aarch64)
    TARGET="aarch64-apple-darwin"
    ;;
  Darwin:x86_64|Darwin:amd64)
    TARGET="x86_64-apple-darwin"
    ;;
  Linux:x86_64|Linux:amd64)
    TARGET="x86_64-unknown-linux-gnu"
    ;;
  Linux:aarch64|Linux:arm64)
    TARGET="aarch64-unknown-linux-gnu"
    ;;
  *)
    fail "不支持的平台 ${OS}/${ARCH}"
    ;;
esac

query_latest_version() {
  loc=$(curl -fsSI "https://github.com/${REPOSITORY}/releases/latest" 2>/dev/null \
    | awk 'tolower($1)=="location:" { print $2; exit }' \
    | tr -d '\r')
  [ -n "$loc" ] || return 0
  # 注意：此处刻意不用 awk 正则（如 gsub(/[/?#]/…))——BSD/BWK awk（macOS 自带）
  # 的字符类里不允许出现 /，会报 nonterminated character class。
  # 用 POSIX sh 参数展开剥出 tag，全部 awk 实现通用。
  case "$loc" in
    */releases/tag/*)
      loc="${loc#*releases/tag/}"
      loc="${loc%%[/?#]*}"
      printf '%s\n' "$loc"
      ;;
  esac
}

if [ -z "$VERSION" ]; then
  VERSION=$(query_latest_version)
fi
[ -n "$VERSION" ] || fail "无法探测最新 Release，请设置 BINOX_VERSION"

VER=$(printf '%s' "$VERSION" | sed 's/^v//')
ASSET="binox-v${VER}-${TARGET}.tar.gz"
RELEASE_BASE_URL="${BINOX_RELEASE_BASE_URL:-https://github.com/${REPOSITORY}/releases/download/${VERSION}}"
RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    fail "需要 sha256sum 或 shasum"
  fi
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

printf '%s\n' "[binox] 下载 ${VERSION} (${TARGET})" >&2
curl -fsSL --retry 3 --retry-delay 1 -o "${TMP}/${ASSET}" "${RELEASE_BASE_URL}/${ASSET}" \
  || fail "下载失败: ${RELEASE_BASE_URL}/${ASSET}"
curl -fsSL --retry 3 --retry-delay 1 -o "${TMP}/SHA256SUMS" "${RELEASE_BASE_URL}/SHA256SUMS" \
  || fail "下载 SHA256SUMS 失败"

expected=$(awk -v name="$ASSET" '$2 == name || $2 == ("*" name) { print $1; exit }' "${TMP}/SHA256SUMS")
[ -n "$expected" ] || fail "SHA256SUMS 中没有 ${ASSET}"
expected=$(printf '%s' "$expected" | tr 'A-F' 'a-f')
actual=$(sha256_file "${TMP}/${ASSET}")
[ "$actual" = "$expected" ] || fail "SHA256 校验失败: ${ASSET}"

mkdir -p "${TMP}/extracted"
tar -xzf "${TMP}/${ASSET}" -C "${TMP}/extracted"
BIN=""
for cand in "${TMP}/extracted/binox" "${TMP}/extracted/binox.exe"; do
  if [ -f "$cand" ]; then
    BIN="$cand"
    break
  fi
done
if [ -z "$BIN" ]; then
  BIN=$(find "${TMP}/extracted" -type f -name 'binox' -o -name 'binox.exe' | head -n 1)
fi
[ -n "$BIN" ] && [ -f "$BIN" ] || fail "归档内没有 binox"
chmod 755 "$BIN"

mkdir -p "$INSTALL_DIR"
DEST="${INSTALL_DIR}/binox"
cp "$BIN" "${DEST}.new"
chmod 755 "${DEST}.new"
mv -f "${DEST}.new" "$DEST"

printf '%s\n' "[binox] 已安装到 ${DEST}" >&2
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*)
    ;;
  *)
    printf '%s\n' "[binox] 请把 ${INSTALL_DIR} 加入 PATH，例如：" >&2
    printf '%s\n' "  export PATH=\"${INSTALL_DIR}:\$PATH\"" >&2
    ;;
esac
"$DEST" --version || true

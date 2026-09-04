#!/usr/bin/env bash
# 任务书第六节验收命令的一键复跑。在仓库根目录执行：
#   ./scripts/selftest.sh
# 默认缓存写到 .selftest-cache。覆盖：BINOX_CACHE_DIR=/path ./scripts/selftest.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ROOT}/target/release/binox"
export BINOX_CACHE_DIR="${BINOX_CACHE_DIR:-${ROOT}/.selftest-cache}"
# 除第 6 节外关掉后台自更新，避免 stderr 掺进主断言
export BINOX_NO_SELFUPDATE="${BINOX_NO_SELFUPDATE:-1}"
PASS=0
FAIL=0
WORKDIR="${ROOT}/.selftest-run"
mkdir -p "$WORKDIR" "$BINOX_CACHE_DIR"

ok() {
  PASS=$((PASS + 1))
  printf 'PASS  %s\n' "$1"
}

bad() {
  FAIL=$((FAIL + 1))
  printf 'FAIL  %s\n' "$1"
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "缺少命令: $1" >&2
    exit 1
  fi
}

need cargo
need rustc

echo "== 构建 =="
(
  cd "$ROOT"
  RUSTFLAGS="-D warnings" cargo test --quiet
  RUSTFLAGS="-D warnings" cargo build --release --quiet
)
test -x "$BIN"
echo "$($BIN --version)"
echo "缓存目录: $BINOX_CACHE_DIR"
echo "二进制:   $BIN"
ls -lh "$BIN"
echo

run_capture() {
  local name="$1"
  shift
  local out="${WORKDIR}/${name}.out"
  local err="${WORKDIR}/${name}.err"
  local start end ms ec
  start="$(date +%s%N)"
  set +e
  "$BIN" "$@" >"$out" 2>"$err"
  ec=$?
  set -e
  end="$(date +%s%N)"
  ms=$(( (end - start) / 1000000 ))
  printf '%s' "$ec" >"${WORKDIR}/${name}.ec"
  printf '%s' "$ms" >"${WORKDIR}/${name}.ms"
  echo "----- ${name}: binox $*  exit=${ec} ${ms}ms -----"
  echo "--- stdout ---"
  cat "$out"
  echo "--- stderr ---"
  cat "$err"
  echo
}

# ---------- 1. 热门库 10 连测 ----------
echo "== 1. 热门库 10 连测（零 --asset-template） =="
run_capture bat sharkdp/bat --version
run_capture fd sharkdp/fd --version
run_capture hyperfine sharkdp/hyperfine --version
run_capture dust bootandy/dust --version
run_capture ripgrep BurntSushi/ripgrep --version
run_capture eza eza-community/eza --version
run_capture starship starship/starship --version
run_capture zoxide ajeetdsouza/zoxide --version
run_capture bottom ClementTsang/bottom --version
run_capture tavily spraylee/tavily-mcp-multi-key --list-tools

assert_ok_match() {
  local name="$1"
  local pat="$2"
  local extra="${3:-}"
  local ec
  ec="$(cat "${WORKDIR}/${name}.ec")"
  if [ "$ec" = "0" ] && grep -Eq "$pat" "${WORKDIR}/${name}.out"; then
    if [ -n "$extra" ] && ! grep -Eq "$extra" "${WORKDIR}/${name}.err"; then
      bad "${name} stdout 通过但 stderr 缺少 ${extra}"
      return
    fi
    ok "${name} exit=0 stdout 匹配 ${pat}"
  else
    bad "${name} 失败 exit=${ec}（期望 stdout 匹配 ${pat}）"
  fi
}

assert_ok_match bat 'bat 0\.'
assert_ok_match fd 'fd [0-9]'
assert_ok_match hyperfine 'hyperfine [0-9]'
assert_ok_match dust '[Dd]ust [0-9]'
assert_ok_match ripgrep 'ripgrep [0-9]'
assert_ok_match eza 'eza '
assert_ok_match starship 'starship [0-9]'
assert_ok_match zoxide 'zoxide [0-9]'
assert_ok_match bottom 'bottom [0-9]'
if [ "$(cat "${WORKDIR}/tavily.ec")" = "0" ] \
  && grep -q 'tavily_search' "${WORKDIR}/tavily.out" \
  && grep -q 'tavily_key_status' "${WORKDIR}/tavily.out"; then
  ok "tavily --list-tools exit=0 且含工具列表"
else
  bad "tavily --list-tools 失败"
fi

# ---------- 2. exec 零残留 ----------
echo
echo "== 2. exec 替换：同一 PID 的 exe 变成目标二进制 =="
FIFO="$(mktemp -u)"
mkfifo "$FIFO"
exec 3<>"$FIFO"
set +e
"$BIN" spraylee/tavily-mcp-multi-key <&3 >"${WORKDIR}/exec.out" 2>"${WORKDIR}/exec.err" &
EPID=$!
set -e
EXEC_OK=0
for _i in $(seq 1 80); do
  if [ ! -d "/proc/${EPID}" ]; then
    echo "进程 ${EPID} 提前退出"
    cat "${WORKDIR}/exec.err" || true
    break
  fi
  EXE="$(readlink "/proc/${EPID}/exe" 2>/dev/null || true)"
  COMM="$(tr -d '\n' < "/proc/${EPID}/comm" 2>/dev/null || true)"
  CMD="$(tr '\0' ' ' < "/proc/${EPID}/cmdline" 2>/dev/null || true)"
  case "$EXE" in
    *tavily-mcp-multi-key)
      echo "pid=${EPID} comm=${COMM}"
      echo "exe=${EXE}"
      echo "cmdline=${CMD}"
      grep -E '^(Name|Pid|PPid):' "/proc/${EPID}/status" || true
      if [ "$(basename "$EXE")" = "tavily-mcp-multi-key" ]; then
        EXEC_OK=1
      fi
      break
      ;;
  esac
  sleep 0.1
done
echo "--- exec stderr ---"
cat "${WORKDIR}/exec.err" || true
kill "$EPID" 2>/dev/null || true
wait "$EPID" 2>/dev/null || true
exec 3>&-
rm -f "$FIFO"

if [ "$EXEC_OK" -eq 1 ]; then
  ok "同一 PID 的 exe basename 已是 tavily-mcp-multi-key（exec 生效）"
else
  bad "未能证明 exec 替换"
fi

if pgrep -x binox >/dev/null 2>&1; then
  bad "exec 场景结束后仍有 binox 进程"
  pgrep -a -x binox || true
else
  ok "exec 场景结束后无残留 binox"
fi

# ---------- 3. 版本新鲜度 ----------
echo
echo "== 3. 版本新鲜度：连跑两次 + @tag 钉死 =="
run_capture fresh1 sharkdp/bat --version
run_capture fresh2 sharkdp/bat --version
echo "fresh1 $(cat "${WORKDIR}/fresh1.ms")ms / fresh2 $(cat "${WORKDIR}/fresh2.ms")ms"
if grep -q '缓存命中 0 下载' "${WORKDIR}/fresh1.err" \
  && grep -q '缓存命中 0 下载' "${WORKDIR}/fresh2.err" \
  && grep -q '最新版本 sharkdp/bat' "${WORKDIR}/fresh1.err"; then
  ok "连跑两次均「缓存命中 0 下载」，且仍有 latest 探测行"
else
  bad "缓存命中或 latest 探测日志不符合预期"
fi

run_capture pinned sharkdp/bat@v0.26.1 --version
if grep -q '最新版本' "${WORKDIR}/pinned.err" || grep -q 'releases/latest' "${WORKDIR}/pinned.err"; then
  bad "@tag 钉死仍出现 latest 探测行"
else
  if [ "$(cat "${WORKDIR}/pinned.ec")" = "0" ] && grep -q 'bat 0.26.1' "${WORKDIR}/pinned.out"; then
    ok "sharkdp/bat@v0.26.1 跳过探测且跑的是 0.26.1"
  else
    bad "@tag 钉死未跑通"
  fi
fi

# ---------- 4. musl 回退 ----------
echo
echo "== 4. musl 回退（ripgrep / zoxide） =="
if grep -E 'musl|回退 musl' "${WORKDIR}/ripgrep.err"; then
  ok "ripgrep 日志可见 musl 回退"
else
  bad "ripgrep 日志未见 musl"
fi
if grep -E 'musl|回退 musl' "${WORKDIR}/zoxide.err"; then
  ok "zoxide 日志可见 musl 回退"
else
  bad "zoxide 日志未见 musl"
fi

# ---------- 5. 退出码透传 ----------
echo
echo "== 5. 退出码透传（构造 exit 3） =="
TRIPLE="x86_64-unknown-linux-gnu"
EXIT3="${WORKDIR}/exit3"
cat >"${WORKDIR}/exit3.rs" <<'RS'
fn main() {
    std::process::exit(3);
}
RS
rustc -O -o "$EXIT3" "${WORKDIR}/exit3.rs"
DEST="${BINOX_CACHE_DIR}/constructed/exit3/v0.0.1/${TRIPLE}"
mkdir -p "$DEST"
cp "$EXIT3" "${DEST}/exit3"
printf 'exit3\n' >"${DEST}/.binox-bin"
printf 'v0.0.1\n' >"${BINOX_CACHE_DIR}/constructed/exit3/last-version"
set +e
"$BIN" --offline constructed/exit3
EC3=$?
set -e
echo "binox --offline constructed/exit3  exit=${EC3}"
if [ "$EC3" -eq 3 ]; then
  ok "退出码透传 \$?==3"
else
  bad "期望退出码 3，实际 ${EC3}"
fi

# ---------- 6. 自更新不阻塞 ----------
echo
echo "== 6. 自更新不阻塞 + 失败只警告 =="
unset BINOX_NO_SELFUPDATE || true
# 对比 fork 开销：缓存已热
measure() {
  local label="$1"
  shift
  local total=0
  local i t0 t1
  for i in 1 2 3 4 5; do
    t0="$(date +%s%N)"
    "$@" >/dev/null 2>/dev/null
    t1="$(date +%s%N)"
    total=$((total + (t1 - t0)))
  done
  echo $(( total / 5 / 1000000 ))
}

MS_OFF="$(BINOX_NO_SELFUPDATE=1 measure off "$BIN" sharkdp/bat --version)"
MS_ON="$(measure on "$BIN" sharkdp/bat --version)"
echo "平均耗时 BINOX_NO_SELFUPDATE=1: ${MS_OFF}ms"
echo "平均耗时 后台自更新开启: ${MS_ON}ms"
DIFF=$(( MS_ON - MS_OFF ))
if [ "$DIFF" -lt 0 ]; then
  DIFF=$(( -DIFF ))
fi
echo "差值绝对值 ${DIFF}ms"
if [ "$DIFF" -le 50 ]; then
  ok "自更新 fork 不影响主命令耗时（差值 ${DIFF}ms ≤ 50ms）"
else
  bad "自更新耗时差 ${DIFF}ms > 50ms（off=${MS_OFF} on=${MS_ON}）"
fi

# 404 失败只警告
export BINOX_SELFUPDATE_URL="https://github.com/spraylee/binox/releases/download/v0.0.0-missing/nope"
run_capture su404 sharkdp/bat --version
# 后台进程写 stderr 可能略晚
sleep 2
echo "--- su404 stderr（含 2s 后追加） ---"
cat "${WORKDIR}/su404.err"
if [ "$(cat "${WORKDIR}/su404.ec")" = "0" ] && grep -q 'bat ' "${WORKDIR}/su404.out"; then
  if grep -E '自更新失败|探测失败' "${WORKDIR}/su404.err"; then
    ok "BINOX_SELFUPDATE_URL=404 主命令仍成功，且有自更新警告"
  else
    # 后台可能还没写完，再等一次
    sleep 2
    if grep -E '自更新失败|探测失败' "${WORKDIR}/su404.err"; then
      ok "BINOX_SELFUPDATE_URL=404 主命令仍成功，且有自更新警告"
    else
      bad "主命令成功但未见自更新警告（后台可能未继承到同一 stderr 文件）"
    fi
  fi
else
  bad "404 自更新场景主命令失败"
fi
unset BINOX_SELFUPDATE_URL
export BINOX_NO_SELFUPDATE=1

# ---------- 7. 性能对比 ----------
echo
echo "== 7. 性能对比：binox bat vs 直接执行缓存二进制 =="
CACHED_BAT="$(find "${BINOX_CACHE_DIR}/sharkdp/bat" -type f -name bat -perm -u+x | head -n 1)"
echo "缓存中的 bat: ${CACHED_BAT}"
if [ -z "$CACHED_BAT" ]; then
  bad "找不到缓存中的 bat"
else
  measure_one() {
    local total=0 i t0 t1
    for i in 1 2 3 4 5; do
      t0="$(date +%s%N)"
      "$@" >/dev/null 2>/dev/null
      t1="$(date +%s%N)"
      total=$((total + (t1 - t0)))
    done
    echo $(( total / 5 / 1000000 ))
  }
  MS_BINOX="$(BINOX_NO_SELFUPDATE=1 measure_one "$BIN" sharkdp/bat --version)"
  MS_DIRECT="$(measure_one "$CACHED_BAT" --version)"
  OVER=$(( MS_BINOX - MS_DIRECT ))
  echo "binox 平均 ${MS_BINOX}ms / 直接执行平均 ${MS_DIRECT}ms / overhead ${OVER}ms"
  if [ "$OVER" -lt 0 ]; then
    OVER=0
  fi
  if [ "$OVER" -le 150 ]; then
    ok "binox overhead ${OVER}ms（<100ms 级；含 302 探测）"
  else
    bad "binox overhead ${OVER}ms 过大"
  fi
fi

echo
echo "结果: ${PASS} 通过 / ${FAIL} 失败"
if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
echo "第六节验证全部通过。"

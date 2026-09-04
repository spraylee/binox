# binox 真实链路验证记录

日期：2026-09-04  
机器：Linux x86_64，glibc 2.39（`ldd --version` → `ldd (Ubuntu GLIBC 2.39-0ubuntu8.7) 2.39`）  
二进制：`/root/.openclaw/workspace/binox/target/release/binox`（`RUSTFLAGS="-D warnings" cargo build --release`，3.1MB）  
单测：`RUSTFLAGS="-D warnings" cargo test` → **43 passed / 0 failed**（纯函数，不打真实网络）

**没有 mock、没有 fixture 顶替网络。** 下面每段输出都是本机实际跑出来的。验收方在仓库根目录执行 `./scripts/selftest.sh` 即可复跑。

缓存：`BINOX_CACHE_DIR=/root/.openclaw/workspace/binox/.selftest-cache`  
除第 6 节外 `BINOX_NO_SELFUPDATE=1`，避免后台探测掺进主断言。

---

## 构建

```text
$ rustc --version
rustc 1.98.0 (88d9e12ae 2026-08-18)
$ cargo --version
cargo 1.98.0 (797e8a9bc 2026-08-05)
$ RUSTFLAGS="-D warnings" cargo test
running 43 tests
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
$ RUSTFLAGS="-D warnings" cargo build --release
Finished `release` profile [optimized] target(s) in 1m 28s
-rwxr-xr-x 2 root root 3.1M target/release/binox
$ ./target/release/binox --version
binox 0.1.0
```

零警告。

---

## 1. 热门库 10 连测（零 `--asset-template`）

全部 `exit=0`。命令即任务书原文。

### sharkdp/bat `--version`

```text
----- bat: binox sharkdp/bat --version  exit=0 122ms -----
--- stdout ---
bat 0.26.1 (979ba22)
--- stderr ---
[binox] 平台 x86_64-unknown-linux-gnu
[binox] 最新版本 sharkdp/bat: v0.26.1
[binox] 下载 https://github.com/sharkdp/bat/releases/download/v0.26.1/bat-v0.26.1-x86_64-unknown-linux-gnu.tar.gz
[binox] 警告: 未找到 SHA256SUMS 资产，跳过校验
[binox] exec /root/.openclaw/workspace/binox/.selftest-cache/sharkdp/bat/v0.26.1/x86_64-unknown-linux-gnu/bat-v0.26.1-x86_64-unknown-linux-gnu/bat --version
```

### sharkdp/fd `--version`

```text
----- fd: binox sharkdp/fd --version  exit=0 1074ms -----
--- stdout ---
fd 10.5.0
--- stderr ---
[binox] 平台 x86_64-unknown-linux-gnu
[binox] 最新版本 sharkdp/fd: v10.5.0
[binox] 下载 https://github.com/sharkdp/fd/releases/download/v10.5.0/fd-v10.5.0-x86_64-unknown-linux-gnu.tar.gz
[binox] 警告: 未找到 SHA256SUMS 资产，跳过校验
[binox] exec .../fd-v10.5.0-x86_64-unknown-linux-gnu/fd --version
```

### sharkdp/hyperfine `--version`

```text
----- hyperfine: binox sharkdp/hyperfine --version  exit=0 1331ms -----
--- stdout ---
hyperfine 1.20.0
--- stderr ---
[binox] 最新版本 sharkdp/hyperfine: v1.20.0
[binox] 下载 https://github.com/sharkdp/hyperfine/releases/download/v1.20.0/hyperfine-v1.20.0-x86_64-unknown-linux-gnu.tar.gz
[binox] exec .../hyperfine-v1.20.0-x86_64-unknown-linux-gnu/hyperfine --version
```

### bootandy/dust `--version`

```text
----- dust: binox bootandy/dust --version  exit=0 1170ms -----
--- stdout ---
Dust 1.2.5
--- stderr ---
[binox] 最新版本 bootandy/dust: v1.2.5
[binox] 下载 https://github.com/bootandy/dust/releases/download/v1.2.5/dust-v1.2.5-x86_64-unknown-linux-gnu.tar.gz
[binox] exec .../dust-v1.2.5-x86_64-unknown-linux-gnu/dust --version
```

### BurntSushi/ripgrep `--version`

```text
----- ripgrep: binox BurntSushi/ripgrep --version  exit=0 122ms -----
--- stdout ---
ripgrep 15.2.0 (rev e89fff89ac)

features:+pcre2
simd(compile):+SSE2,-SSSE3,-AVX2
simd(runtime):+SSE2,+SSSE3,+AVX2

PCRE2 10.45 is available (JIT is available)
--- stderr ---
[binox] 平台 x86_64-unknown-linux-gnu
[binox] 最新版本 BurntSushi/ripgrep: 15.2.0
[binox] gnu 资产未命中，回退 musl: ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz (x86_64-unknown-linux-musl)
[binox] 下载 https://github.com/BurntSushi/ripgrep/releases/download/15.2.0/ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz
[binox] 警告: 未找到 SHA256SUMS 资产，跳过校验
[binox] exec .../ripgrep-15.2.0-x86_64-unknown-linux-musl/rg --version
```

包内二进制是 `rg`，候选名 `ripgrep` miss 后走「恰好一个原生文件」启发式。

### eza-community/eza `--version`

```text
----- eza: binox eza-community/eza --version  exit=0 1092ms -----
--- stdout ---
eza eza - A modern, maintained replacement for ls
v0.23.5 [+git]
https://github.com/eza-community/eza
--- stderr ---
[binox] 最新版本 eza-community/eza: v0.23.5
[binox] 下载 https://github.com/eza-community/eza/releases/download/v0.23.5/eza_x86_64-unknown-linux-gnu.tar.gz
[binox] exec .../eza --version
```

下划线、无版本（矩阵第 3 条）。

### starship/starship `--version`

```text
----- starship: binox starship/starship --version  exit=0 1364ms -----
--- stdout ---
starship 1.26.0
branch:main
commit_hash:fca92d8
build_time:2026-06-28 17:04:46 +00:00
build_env:rustc 1.96.0 (ac68faa20 2026-05-25),
--- stderr ---
[binox] 最新版本 starship/starship: v1.26.0
[binox] 下载 https://github.com/starship/starship/releases/download/v1.26.0/starship-x86_64-unknown-linux-gnu.tar.gz
[binox] exec .../starship --version
```

无版本连字符（矩阵第 4 条）。

### ajeetdsouza/zoxide `--version`

```text
----- zoxide: binox ajeetdsouza/zoxide --version  exit=0 1111ms -----
--- stdout ---
zoxide 0.10.0
--- stderr ---
[binox] 最新版本 ajeetdsouza/zoxide: v0.10.0
[binox] gnu 资产未命中，回退 musl: zoxide-0.10.0-x86_64-unknown-linux-musl.tar.gz (x86_64-unknown-linux-musl)
[binox] 下载 https://github.com/ajeetdsouza/zoxide/releases/download/v0.10.0/zoxide-0.10.0-x86_64-unknown-linux-musl.tar.gz
[binox] exec .../zoxide --version
```

### ClementTsang/bottom `--version`（bin=`btm`，未传 `--bin`）

```text
----- bottom: binox ClementTsang/bottom --version  exit=0 127ms -----
--- stdout ---
bottom 0.14.9
--- stderr ---
[binox] 最新版本 ClementTsang/bottom: 0.14.9
[binox] 下载 https://github.com/ClementTsang/bottom/releases/download/0.14.9/bottom_x86_64-unknown-linux-gnu.tar.gz
[binox] exec .../btm --version
```

资产名 `bottom_*`，解压后只有 `btm`，启发式自动选中。

### spraylee/tavily-mcp-multi-key `--list-tools`

```text
----- tavily: binox spraylee/tavily-mcp-multi-key --list-tools  exit=0 126ms -----
--- stdout ---
Available tools:

- tavily_search
  ...
- tavily_extract
  ...
- tavily_crawl
  ...
- tavily_map
  ...
- tavily_research
  ...
- tavily_key_status
  ...
--- stderr ---
[binox] 最新版本 spraylee/tavily-mcp-multi-key: v0.4.1
[binox] 下载 https://github.com/spraylee/tavily-mcp-multi-key/releases/download/v0.4.1/tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz
[binox] SHA256 校验通过: tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz
[binox] exec .../tavily-mcp-multi-key --list-tools
```

B 类命名 + SHA256SUMS 校验通过。stdout 含 6 个工具名（全文见 `./scripts/selftest.sh` 复跑）。

---

## 2. exec 零残留

用 fifo 卡住 stdin，后台启动不带 `--list-tools` 的 tavily MCP，抓同一 PID：

```text
pid=3762483 comm=tavily-mcp-mult
exe=/root/.openclaw/workspace/binox/.selftest-cache/spraylee/tavily-mcp-multi-key/v0.4.1/x86_64-unknown-linux-gnu/tavily-mcp-multi-key
cmdline=/root/.openclaw/workspace/binox/.selftest-cache/spraylee/tavily-mcp-multi-key/v0.4.1/x86_64-unknown-linux-gnu/tavily-mcp-multi-key 
Name:	tavily-mcp-mult
Pid:	3762483
PPid:	3762262
```

对应 stderr：

```text
[binox] 平台 x86_64-unknown-linux-gnu
[binox] 最新版本 spraylee/tavily-mcp-multi-key: v0.4.1
[binox] 缓存命中 0 下载: v0.4.1 (x86_64-unknown-linux-gnu)
[binox] exec .../tavily-mcp-multi-key 
[tavily-mcp-multi-key] no Tavily API key set; running in keyless mode. ...
```

结论：

- PID 不变（`3762483`），`/proc/$PID/exe` 已指向缓存里的目标 ELF。这就是 POSIX `exec`：替换映像，不 spawn 子进程。
- `cmdline` 不再含 `binox`。
- Linux `TASK_COMM_LEN=16`，`tavily-mcp-multi-key` 显示为 `tavily-mcp-mult`。以 `exe` + `cmdline` 为准。
- 杀掉该 PID 后 `pgrep -x binox` 为空。

---

## 3. 版本新鲜度

同一缓存上连跑两次 `binox sharkdp/bat --version`：

```text
----- fresh1: ... exit=0 32ms -----
[binox] 最新版本 sharkdp/bat: v0.26.1
[binox] 缓存命中 0 下载: v0.26.1 (x86_64-unknown-linux-gnu)
[binox] exec .../bat --version

----- fresh2: ... exit=0 25ms -----
[binox] 最新版本 sharkdp/bat: v0.26.1
[binox] 缓存命中 0 下载: v0.26.1 (x86_64-unknown-linux-gnu)
[binox] exec .../bat --version
```

仍会做一次 `/releases/latest` 302（日志有「最新版本」），但不下资产。冷启动 bat 122ms（含下载），命中后 25–32ms。

钉死 `@v0.26.1`（日志无 latest 探测行）：

```text
----- pinned: binox sharkdp/bat@v0.26.1 --version  exit=0 9ms -----
--- stdout ---
bat 0.26.1 (979ba22)
--- stderr ---
[binox] 平台 x86_64-unknown-linux-gnu
[binox] 缓存命中 0 下载: v0.26.1 (x86_64-unknown-linux-gnu)
[binox] exec .../bat --version
```

没有「最新版本」行，没有 `releases/latest`。9ms（跳过 302）。

---

## 4. musl 回退

gnu 主机上 ripgrep / zoxide 只发 musl 资产。日志原文：

```text
[binox] gnu 资产未命中，回退 musl: ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz (x86_64-unknown-linux-musl)
[binox] gnu 资产未命中，回退 musl: zoxide-0.10.0-x86_64-unknown-linux-musl.tar.gz (x86_64-unknown-linux-musl)
```

exec 路径分别落在 `.../x86_64-unknown-linux-musl/rg` 与 `.../x86_64-unknown-linux-musl/zoxide`。本机跑通。

---

## 5. 退出码透传

构造缓存包 `constructed/exit3@v0.0.1`（`rustc` 编一个 `exit(3)` 的 ELF，写入 `.binox-bin` + `last-version`）：

```text
[binox] 平台 x86_64-unknown-linux-gnu
[binox] 缓存命中 0 下载: v0.0.1 (x86_64-unknown-linux-gnu)
[binox] exec .../constructed/exit3/v0.0.1/x86_64-unknown-linux-gnu/exit3 
binox --offline constructed/exit3  exit=3
```

`$?` == 3。`--offline` 未发探测。exec 把目标退出码变成进程退出码。

---

## 6. 自更新不阻塞

缓存已热的 `sharkdp/bat --version`，各 5 次平均：

```text
平均耗时 BINOX_NO_SELFUPDATE=1: 25ms
平均耗时 后台自更新开启: 23ms
差值绝对值 2ms
```

差值 2ms，远小于 50ms。主流程 fork 完立即继续。

`BINOX_SELFUPDATE_URL` 指向不存在的资产（HTTP 404），主命令仍成功：

```text
----- su404: binox sharkdp/bat --version  exit=0 26ms -----
--- stdout ---
bat 0.26.1 (979ba22)
--- stderr ---
[binox] 平台 x86_64-unknown-linux-gnu
[binox] 最新版本 sharkdp/bat: v0.26.1
[binox] 缓存命中 0 下载: v0.26.1 (x86_64-unknown-linux-gnu)
[binox] exec .../bat --version
[binox] 警告: 自更新失败: 自更新探测失败: 探测 https://github.com/spraylee/binox/releases/download/v0.0.0-missing/nope 失败: HTTP 404 Not Found
```

主命令 exit=0。后台警告出现在 exec 之后（detached 进程），不影响退出码。同一次等待里警告打了两行（后台 worker 与探测路径各一次），都是警告、都是 404，主命令未失败。

---

## 7. 性能对比

```text
缓存中的 bat: .../bat-v0.26.1-x86_64-unknown-linux-gnu/bat
binox 平均 24ms / 直接执行平均 3ms / overhead 21ms
```

overhead **21ms**（含同步 302 探测），<100ms 级。

---

## 复跑

```sh
./scripts/selftest.sh
```

本次结果：**20 通过 / 0 失败**。第六节 8 条全部真跑通过。

# binox

从 GitHub Release 拉取**当前平台**的最新预编译二进制并执行。npx 的体验，零 node / cargo / npm 依赖。POSIX 上 `exec` 自替换，跑完不残留父进程。

**不查 crates.io，不做 JS 兼容，不编译源码，只服务 GitHub Release。**

## 安装

一行 curl（装到 `~/.local/bin`，没有 PATH 会提示）：

```sh
curl -fsSL https://github.com/spraylee/binox/releases/latest/download/bootstrap.sh | sh
```

钉死版本：

```sh
BINOX_VERSION=v0.1.0 sh scripts/bootstrap.sh
```

已有 cargo-binstall 的用户：

```sh
cargo binstall binox
```

`Cargo.toml` 带 `[package.metadata.binstall]`，`pkg-url` 指向本仓库 Release 的 `binox-v{ver}-{triple}.tar.gz`。

Windows 可用 `scripts/bootstrap.ps1`（优先 `.zip`）。

本仓库的 Release 资产命名是 B 类：`binox-v{ver}-{triple}.tar.gz`。自己能跑自己：

```sh
binox spraylee/binox --version
```

## 用法

```
binox owner/repo [args...]                    # 运行最新版，args 透传
binox owner/repo@v1.2.3 [args...]             # 钉死版本（跳过 latest 探测）
binox --version <tag> owner/repo [args...]    # 同上的显式形式
binox --bin <name> owner/repo [args...]       # 多二进制时选哪个
binox --asset-template '{name}-v{version}-{target}.tar.gz' owner/repo
binox --offline owner/repo [args...]          # 跳过探测，纯缓存
binox update                                  # 前台自更新
binox --help / --version
```

自有 flag 必须写在 target **之前**。target 之后的 token 全部透传（包括对方的 `--version`）。

```sh
binox sharkdp/bat --version
binox BurntSushi/ripgrep -- --version
binox --offline sharkdp/fd --version
binox spraylee/tavily-mcp-multi-key --list-tools
```

## 双链路更新

1. **包同步保新鲜（主语义，同步）**  
   未钉版本时，每次运行都 302 探测 `GET https://github.com/{owner}/{repo}/releases/latest`（读 Location 的 tag，不消耗 API 配额）。tag 没变则「缓存命中 0 下载」再 `exec`；tag 更新则先下载再跑新版。探测超时上限 **3 秒**：有 `last-version` 就回退并打印 `探测失败，使用缓存版本 {tag}`，没有缓存则报错。`@tag` / `--offline` 完全不发探测请求。

2. **自身后台静默**  
   每次跑非 `update` 命令时，fork 一个 detached 后台进程去探测 `spraylee/binox` 的 latest。有新版则下载 + SHA256 + 原子替换（`.new` → rename），再 `binox --version` 冒烟，失败回滚。主流程 fork 完立刻继续，**0 等待**。失败只写一行 stderr 警告，不影响主命令退出码。  
   `BINOX_NO_SELFUPDATE=1` 跳过。`binox update` 走前台同一条链路。

## 资产命名矩阵

先拉 `https://github.com/{o}/{r}/releases/expanded_assets/{tag}` 拿真实文件名（免 API 配额），再按顺序匹配。`{name}` = 仓库名小写，`{ver}` = tag 去掉前导 `v` 后再取**末段 semver**（`cargo-nextest-0.9.143` → `0.9.143`），`{target}` = 平台 triple。

1. `{name}-v{ver}-{target}.tar.gz` / `.tgz`（bat / fd / hyperfine / dust / tavily）
2. `{name}-{ver}-{target}.tar.gz` / `.tgz`（ripgrep / zoxide / just）
3. `{name}_{target}.tar.gz` / `.tgz`（eza / bottom）
4. `{name}-{target}.tar.gz` / `.tgz`（starship）
5. `{name}-{ver}-{target}.zip` / `{name}-v{ver}-{target}.zip`
6. gnu 没命中 → 同一矩阵再试 musl triple（ripgrep / zoxide / just 只发 musl；静态 musl 二进制在 gnu 主机上能跑）
7. 以上全 miss → 模糊匹配：文件名包含 triple 或其等价体（`x86_64-linux` / `amd64-linux` 等），且以 `.tar.gz` / `.tgz` / `.zip` 结尾，不含 `.deb` / `.rpm` / `.msi` / `.sha` / `.sig` / `.json` / `.txt`。唯一命中则用并警告；多个则列出，要求 `--asset-template`
8. `--asset-template` 覆盖一切

`--asset-template` 占位符：

- `{name}`：仓库名小写
- `{version}`：**原始 tag 去掉前导 `v`**（`v1.2.3` → `1.2.3`；`15.2.0` 保持；`cargo-nextest-0.9.143` 保持全文，不剥前缀）
- `{target}`：平台 triple
- `{tag}`：原始 tag 全文

有 `SHA256SUMS` 资产就校验。Windows `.zip` 解压路径已实现。

## 缓存与逃生门

缓存根：`BINOX_CACHE_DIR`，否则 Linux `~/.cache/binox/`，macOS `~/Library/Caches/binox/`，Windows `%LOCALAPPDATA%\binox\`。

布局：`{owner}/{repo}/{tag}/{target}/` + `last-version`。下载走 `.part` + flock，校验后再 rename。

- `--offline`：CI 确定性，不探测、不下新包
- `@tag` / `--version <tag>`：钉死，跳过 latest
- `--bin` / `--asset-template`：名字对不上时的逃生门
- `BINOX_NO_SELFUPDATE=1`：关掉后台自更新
- `BINOX_SELFUPDATE_URL`：覆盖自更新探测 URL（测试用）

## 和别人的差异

| 工具 | 做什么 | 要不要本机工具链 | 跑完还在不在 |
|---|---|---|---|
| npx / bunx | 跑 npm 包（常是 JS） | 要 node / bun | 通常留一个 node 父进程 |
| cargo-binstall | **安装器**，写进 `~/.cargo/bin` | 不要 rustc，但定位是「装上」 | 装完就留下 |
| pkgxdev/cargox | 包一层 cargo-binstall / cargo install | 经常还是要 cargo | 按需安装再跑 |
| **binox** | **运行器**：GitHub Release → 缓存 → `exec` | 不要 node / cargo / rustc | POSIX 零残留 |

找不到当前平台的预编译包就报错退出，**不会**回退编译，也**不会**执行任何 install 脚本。

## For AI agents

This repo ships a skill (root `SKILL.md`) and a live-docs endpoint, so agents
never rely on stale instructions:

```sh
npx skills add spraylee/binox -g          # install the thin-pointer skill
curl -fsSL https://binox.spraylee.com/llms.txt   # always-fresh docs
```

The skill is deliberately **thin** (single file, rarely changes) and points
here for anything version-specific. `npx skills update` keeps it in sync with
the repo's main branch.

## 开发

```sh
RUSTFLAGS="-D warnings" cargo test
RUSTFLAGS="-D warnings" cargo build --release
./scripts/selftest.sh
```

真实链路证据见 `VERIFICATION.md`。

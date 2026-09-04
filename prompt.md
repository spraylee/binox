# binox — GitHub Release 二进制的零配置运行器（终版开发任务书）

你是资深 Rust 工程师。在 /root/.openclaw/workspace/binox（本目录，全新工程）开发 **binox** 并完成真实链路验证。原型代码在 /root/.openclaw/workspace/mynpx/（只读参考，70-80% 可复用：github.rs/cache.rs/runner.rs/platform.rs/http.rs/archive.rs/locate.rs/cli.rs），调研报告在 /root/.openclaw/workspace/mynpx/CARGOX_RESEARCH.md（资产命名模式实测数据，必读）。禁止虚报验证结果——验收方会独立复跑。

## 一、定位（一句话）

`binox owner/repo` = 从 GitHub Release 拉取**当前平台**的最新预编译二进制并执行（POSIX exec 自替换零残留）。npx 的体验，零 node/cargo/npm 依赖。**不查 crates.io，不做 JS 兼容，不编译，只服务 GitHub Release。**

## 二、命令行规格

```
binox owner/repo [args...]                    # 运行最新版，args 透传
binox owner/repo@v1.2.3 [args...]             # 钉死版本（跳过 latest 探测）
binox --version <tag> owner/repo [args...]    # 同上的显式形式
binox --bin <name> owner/repo [args...]       # 多二进制时选哪个
binox --asset-template '{name}-...-{target}.tar.gz' owner/repo   # 匹配兜底
binox --offline owner/repo [args...]          # 跳过探测纯缓存（CI 确定性）
binox update                                  # 显式自更新
binox --help / --version
```
自有 flag 必须在 target 之前；target 之后的 token 全部透传给被运行程序（含 --version 等）。

## 三、核心行为规格（逐条实现）

### 3.1 版本新鲜度：同步探测（不是后台！这是主语义）

1. 未钉版本时：同步 302 探测 `GET https://github.com/{o}/{r}/releases/latest`（读 Location 的 tag，零 API 配额，参考原型 github.rs）
2. tag == 缓存版本 → 「缓存命中 0 下载」日志 → 直接 exec
3. tag 更新 → 同步下载新版 → 校验 → exec 新版（这次等待是语义必需的）
4. **探测超时上限 3 秒**（断网/慢网）：回退缓存版（有 last-version 记录时）+ stderr 一行警告 `探测失败，使用缓存版本 {tag}`；无缓存则报错退出
5. `@version` 钉死 / `--offline` → 完全不发探测请求

### 3.2 资产匹配矩阵（本任务的核心新功能）

原型只支持单模板，实测 13 个热门库只中 5 个。你要实现**列举式匹配**：先从 `https://github.com/{o}/{r}/releases/expanded_assets/{tag}` 拿真实资产名列表（免 API 配额，原型的做法，保留），然后按以下候选矩阵依次匹配（吸收 cargo-binstall 默认矩阵，见 CARGOX_RESEARCH.md 任务3）：

文件名候选（{name}=仓库名小写, {ver}=tag去掉前导v, {target}=平台triple）：
1. `{name}-v{ver}-{target}.tar.gz|tgz` （bat/fd/hyperfine/dust/我们的 tavily）
2. `{name}-{ver}-{target}.tar.gz|tgz` （ripgrep/zoxide/just）
3. `{name}_{target}.tar.gz|tgz` （eza/bottom，下划线无版本）
4. `{name}-{target}.tar.gz|tgz` （starship 无版本）
5. `{name}-{ver}-{target}.zip` / `{name}-v{ver}-{target}.zip` （部分项目压缩包用 zip）
6. gnu 没命中 → 同矩阵再试 musl triple（ripgrep/zoxide/just 只发 musl；musl 静态二进制 gnu 主机能跑，实测已证）
7. tag 带前缀的仓库（如 `cargo-nextest-0.9.143`）：{ver} 取 tag 的**末段 semver**（`0.9.143`），再跑矩阵
8. 以上全 miss → 兜底模糊匹配：资产名包含 target triple（或其等价体 x86_64-linux/amd64-linux 等）且以 .tar.gz/.tgz/.zip 结尾且不含 .deb/.rpm/.msi/.sha/.sig/.json/.txt → 若唯一命中则用并打警告日志；多个则列出让用户 --asset-template
9. `--asset-template` 显式覆盖一切（占位符 {name} {version} {target}，{version} 用原始 tag 语义要文档写清）
10. SHA256SUMS 资产存在则校验（原型已有）；Windows .zip 解压路径写好（本机不测）

### 3.3 bin 定位启发式（解压后找哪个可执行文件）

1. 按 `--bin` 指定名找（无扩展名）
2. 无指定：取候选名列表 = [仓库名小写, 资产名前缀（第一个 `-` 或 `_` 前）] 在解压根目录与一级子目录递归找匹配的可执行文件；Mach-O/ELF 用魔数校验（原型 locate.rs 已有），跳过 .sha/.txt
3. 找到多个：全部列出 + 提示 --bin；恰好一个：直接用
4. 典型案例要能自动过：ripgrep 包内目录带 `rg`、bottom 的二进制叫 `btm`（资产名前缀 bottom 匹配不到时，若解压后恰好只有一个可执行文件则用它）

### 3.4 自更新：后台静默，绝不阻塞主功能

1. 每次运行非 update 命令时：fork 一个 **detached 后台子进程**（POSIX: setsid + double-fork 或等价；stdin/out/err 不继承主流程管道），它去探测 `github.com/spraylee/binox/releases/latest`，有新版则下载+SHA256校验+替换自己（写临时文件 → rename 原子覆盖，运行中的旧进程不受影响，参考 tavily bootstrap 的 .new→mv 模式），替换后跑 `binox --version` 冒烟，失败回滚
2. **主流程 fork 完立即继续 exec，0 等待**；后台进程任何失败只写一行 stderr 警告，不影响主命令退出码
3. `BINOX_NO_SELFUPDATE=1` 环境变量：跳过自更新 fork
4. `binox update`：前台显式自更新（同样原子替换+冒烟+回滚）
5. 自身下载源就是自己的 GitHub Release（dogfood 自己的核心能力）
6. 冷启动优化：启动时若离线（本机无网络）探测失败不要重试超过 1 次

### 3.5 缓存与并发

- 缓存根：`BINOX_CACHE_DIR` 或 `~/.cache/binox/`（macOS: `~/Library/Caches/binox/`，Windows: `%LOCALAPPDATA%\binox\`）
- 布局 `{owner}/{repo}/{tag}/{target}/` + `last-version` 文件；flock 并发锁；.part 临时下载 → 校验 → rename 原子落位；损坏缓存检测重下（原型 cache.rs 已有，复用）
- 环境变量统一 `BINOX_` 前缀

## 四、安装与分发（文档+CI 写好，不实际发布）

1. `scripts/bootstrap.sh` / `.ps1`：一键安装（curl 拉自己 Release → 校验 → 装到 ~/.local/bin + PATH 提示），参考 tavily 项目的 bootstrap（/root/.openclaw/workspace/tavily-mcp-multi-key/scripts/bootstrap.sh）
2. Cargo.toml 带 `[package.metadata.binstall]` 段（pkg-url 指向自己的 Release 资产，让 cargo-binstall 用户白送）
3. `.github/workflows/release.yml`：多平台构建（linux x64/arm64 + macOS x64/arm64 + Windows x64）+ tar.gz/tgz/zip + SHA256SUMS + bootstrap 资产 + 自动 Release（参考 tavily 项目的 release.yml，路径 /root/.openclaw/workspace/tavily-mcp-multi-key/.github/workflows/release.yml）
4. Release 资产命名必须自洽：`binox-v{ver}-{triple}.tar.gz`（走我们自己的 B 类模板，自己的工具必须能跑自己：`binox spraylee/binox --version` 要通——这是最强 dogfood）
5. README.md：安装（curl 一行 + binstall）/ 用法 / 双链路更新哲学（包同步保新鲜 + 自身后台静默）/ 支持的资产命名矩阵 / --offline 与逃生门 / 与 npx·bunx·cargo-binstall·pkgxdev/cargox 的差异表

## 五、工程要求

- 纯 Rust，crate 名 `binox`，bin 名 `binox`；依赖克制（沿用原型的：reqwest/tar/flate2/zip/sha2/serde_json/clap 或 lexopt/fd-lock）
- `RUSTFLAGS="-D warnings" cargo build --release` 零警告；`cargo test` 全绿（单测覆盖：匹配矩阵逐条、triple 映射、musl 回退、tag 前缀剥离、bin 启发式、版本解析——纯函数不打网络）
- git init 全新历史，分阶段 commit（骨架/匹配矩阵/bin 启发式/自更新/验证）
- **不 push、不建 GitHub Release、不发布**（本地开发+验证即可，发布由老板另行决定）

## 六、验证要求（真实网络，全部记录进 VERIFICATION.md，任何失败修到通过）

1. **热门库 10 连测（核心验收）**——全部零配置（不带 --asset-template）跑通 `--version` 或等价命令：
   `sharkdp/bat`、`sharkdp/fd`、`sharkdp/hyperfine`、`bootandy/dust`、`BurntSushi/ripgrep`、`eza-community/eza`、`starship/starship`、`ajeetdsouza/zoxide`、`ClementTsang/bottom`（bin=btm）、`spraylee/tavily-mcp-multi-key`（--list-tools）
2. **exec 零残留铁证**：跑 tavily（长命进程，stdin 挂住），抓 /proc 进程树证明 binox 已被 exec 替换（原型验证脚本模式可参考 mynpx/VERIFICATION.md）
3. **版本新鲜度**：同一库连跑两次 → 第二次「缓存命中 0 下载」；用 `@tag` 钉死验证跳过探测（日志无 latest 探测行）
4. **musl 回退**：ripgrep/zoxide 在 gnu 主机上走 musl 资产（日志可见）
5. **退出码透传**：构造一个退出码 3 的场景验证 `$?`==3
6. **自更新不阻塞**：验证后台 fork 不影响主命令耗时（对比 BINOX_NO_SELFUPDATE=1 的耗时差异 <50ms 级）；自更新失败场景（如 BINOX_SELFUPDATE_URL 指向 404）只出警告不影响主命令
7. **性能对比**：任一热门库（如 bat）对比直接执行缓存二进制的 overhead（应 <100ms 级）
8. 全部命令的完整输出贴 VERIFICATION.md，附 `scripts/selftest.sh` 一键复跑

## 七、禁止事项

- 不引入 node/npm/cargo 生态运行时依赖；不查 crates.io；不做 JS 包；不编译源码
- 不执行任何包的 install 脚本（我们是运行器不是安装器）
- 不修改 /root/.openclaw/workspace/mynpx/ 和 /root/.openclaw/workspace/tavily-mcp-multi-key/ 下任何文件（只读参考）
- 不 push 不发布不装系统路径
- 敏感信息（API key）不进代码/日志/文档（全程无需任何 key）
- 不虚报验证结果

## 八、环境提示

本机 Linux x64 glibc、cargo 1.98 已装、网络通 GitHub。tavily 的 Release（spraylee/tavily-mcp-multi-key）是 B 类命名+SHA256SUMS+bootstrap 资产齐全的样板仓库，可当首要测试对象。CARGOX_RESEARCH.md 里 2.2 节有 13 个库的资产命名实测表，直接查表开发。

开始吧。先读 CARGOX_RESEARCH.md 2.2/3.2 节 + mynpx 源码结构，然后按 二→三→五→六 推进。完成标准 = 第六节 8 条全真通过 + VERIFICATION.md 证据齐全。

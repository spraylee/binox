use crate::error::{Error, Result};

/// 顶层命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Update,
    Run(Cli),
}

/// 解析后的运行命令。binox 自己的选项只能出现在 `<target>` 之前，
/// 之后的所有参数（含 `--version`）原样透传给目标二进制。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub version: Option<String>,
    pub bin: Option<String>,
    pub asset_template: Option<String>,
    pub offline: bool,
    pub owner: String,
    pub repo: String,
    pub args: Vec<String>,
}

pub fn parse_args<I, S>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let raw: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    if raw.is_empty() {
        return Err(Error::with_code(2, "缺少 <target>。查看用法：binox --help"));
    }
    if raw.len() == 1 && (raw[0] == "--help" || raw[0] == "-h") {
        return Ok(Command::Help);
    }
    if raw.len() == 1 && (raw[0] == "--version" || raw[0] == "-V") {
        return Ok(Command::Version);
    }
    if raw[0] == "update" {
        if raw.len() > 1 {
            return Err(Error::with_code(2, "update 不接受额外参数"));
        }
        return Ok(Command::Update);
    }

    let mut version = None;
    let mut bin = None;
    let mut asset_template = None;
    let mut offline = false;
    let mut iter = raw.into_iter().peekable();

    while let Some(head) = iter.peek().cloned() {
        if head == "--help" || head == "-h" {
            return Ok(Command::Help);
        }
        if head == "--" {
            iter.next();
            break;
        }
        if head == "--offline" {
            iter.next();
            offline = true;
            continue;
        }
        if let Some(value) = take_option(&mut iter, "--version")? {
            version = Some(value);
            continue;
        }
        if let Some(value) = take_option(&mut iter, "--bin")? {
            bin = Some(value);
            continue;
        }
        if let Some(value) = take_option(&mut iter, "--asset-template")? {
            asset_template = Some(value);
            continue;
        }
        if head.starts_with('-') {
            return Err(Error::with_code(
                2,
                format!("未知选项 {head}。查看用法：binox --help"),
            ));
        }
        break;
    }

    let target = iter
        .next()
        .ok_or_else(|| Error::with_code(2, "缺少 <target>。查看用法：binox --help"))?;
    if target.is_empty() {
        return Err(Error::with_code(2, "target 不能为空"));
    }
    let (owner, repo, at_version) = parse_owner_repo(&target)?;
    if version.is_none() {
        version = at_version;
    }
    let rest = iter.collect();
    Ok(Command::Run(Cli {
        version,
        bin,
        asset_template,
        offline,
        owner,
        repo,
        args: rest,
    }))
}

/// `owner/repo` 或 `owner/repo@tag`。
pub fn parse_owner_repo(target: &str) -> Result<(String, String, Option<String>)> {
    let (path, at_version) = match target.split_once('@') {
        Some((path, ver)) => {
            if ver.is_empty() {
                return Err(Error::with_code(2, format!("无效的版本钉死 {target}")));
            }
            (path, Some(ver.to_string()))
        }
        None => (target, None),
    };
    let Some((owner, repo)) = path.split_once('/') else {
        return Err(Error::with_code(
            2,
            format!("无效的 GitHub 目标 {target}（期望 owner/repo 或 owner/repo@tag）"),
        ));
    };
    if owner.is_empty() || repo.is_empty() || repo.contains('/') || owner.starts_with('@') {
        return Err(Error::with_code(
            2,
            format!("无效的 GitHub 目标 {target}（期望 owner/repo）"),
        ));
    }
    Ok((owner.to_string(), repo.to_string(), at_version))
}

fn take_option<I>(iter: &mut std::iter::Peekable<I>, name: &str) -> Result<Option<String>>
where
    I: Iterator<Item = String>,
{
    let Some(raw) = iter.peek().cloned() else {
        return Ok(None);
    };
    if raw == name {
        iter.next();
        let value = iter
            .next()
            .ok_or_else(|| Error::with_code(2, format!("{name} 需要一个参数")))?;
        return Ok(Some(value));
    }
    let prefix = format!("{name}=");
    if let Some(value) = raw.strip_prefix(&prefix) {
        iter.next();
        if value.is_empty() {
            return Err(Error::with_code(2, format!("{name} 需要一个参数")));
        }
        return Ok(Some(value.to_string()));
    }
    Ok(None)
}

pub fn print_help() {
    print!(
        "\
binox {version} — GitHub Release 二进制的零配置运行器

用法:
  binox owner/repo [args...]
  binox owner/repo@v1.2.3 [args...]
  binox --version <tag> owner/repo [args...]
  binox --bin <name> owner/repo [args...]
  binox --asset-template '{{name}}-v{{version}}-{{target}}.tar.gz' owner/repo
  binox --offline owner/repo [args...]
  binox update
  binox --help / --version

选项（必须写在 target 前面；后面的参数全部透传，含 --version）:
  --version <tag>          钉死 Release tag（跳过 latest 探测）
  --bin <name>             多二进制时指定入口
  --asset-template <tpl>   覆盖资产匹配；占位符 {{name}} {{version}} {{target}} {{tag}}
  --offline                不发探测请求，只用 last-version 缓存
  -h, --help               显示帮助
  -V, --version            打印 binox 自身版本（单独使用时）

环境变量:
  BINOX_CACHE_DIR          缓存根（默认 ~/.cache/binox，macOS ~/Library/Caches/binox）
  BINOX_NO_SELFUPDATE=1    跳过后台自更新
  BINOX_SELFUPDATE_URL     覆盖自更新探测 URL（测试用）
  BINOX_SELFUPDATE_REPO    覆盖自更新仓库（默认 spraylee/binox）

占位符 {{version}}：原始 tag 去掉前导 v（v1.2.3 → 1.2.3）。带 crate 前缀的 tag 请用 {{tag}} 或字面量。
",
        version = env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_version_after_target() {
        let Command::Run(cli) = parse_args(["sharkdp/bat", "--version"]).unwrap() else {
            panic!("expected run");
        };
        assert_eq!(cli.owner, "sharkdp");
        assert_eq!(cli.repo, "bat");
        assert_eq!(cli.version, None);
        assert_eq!(cli.args, vec!["--version"]);
    }

    #[test]
    fn pin_version_before_target() {
        let Command::Run(cli) =
            parse_args(["--version", "v0.4.1", "spraylee/tavily-mcp-multi-key", "--list-tools"])
                .unwrap()
        else {
            panic!("expected run");
        };
        assert_eq!(cli.version.as_deref(), Some("v0.4.1"));
        assert_eq!(cli.owner, "spraylee");
        assert_eq!(cli.args, vec!["--list-tools"]);
    }

    #[test]
    fn at_syntax_and_offline() {
        let Command::Run(cli) =
            parse_args(["--offline", "BurntSushi/ripgrep@15.2.0", "-n", "foo"]).unwrap()
        else {
            panic!("expected run");
        };
        assert!(cli.offline);
        assert_eq!(cli.version.as_deref(), Some("15.2.0"));
        assert_eq!(cli.owner, "BurntSushi");
        assert_eq!(cli.repo, "ripgrep");
        assert_eq!(cli.args, vec!["-n", "foo"]);
    }

    #[test]
    fn equals_form_and_bin() {
        let Command::Run(cli) =
            parse_args(["--bin=btm", "--version=0.14.9", "ClementTsang/bottom"]).unwrap()
        else {
            panic!("expected run");
        };
        assert_eq!(cli.bin.as_deref(), Some("btm"));
        assert_eq!(cli.version.as_deref(), Some("0.14.9"));
    }

    #[test]
    fn bare_version_and_update() {
        assert_eq!(parse_args(["--version"]).unwrap(), Command::Version);
        assert_eq!(parse_args(["-V"]).unwrap(), Command::Version);
        assert_eq!(parse_args(["update"]).unwrap(), Command::Update);
        assert!(parse_args(["update", "extra"]).is_err());
    }

    #[test]
    fn flag_version_overrides_at() {
        let Command::Run(cli) =
            parse_args(["--version", "v0.26.1", "sharkdp/bat@v0.1.0"]).unwrap()
        else {
            panic!("expected run");
        };
        assert_eq!(cli.version.as_deref(), Some("v0.26.1"));
    }

    #[test]
    fn rejects_bare_name() {
        assert!(parse_args(["ripgrep"]).is_err());
    }

    #[test]
    fn parse_owner_repo_cases() {
        let (o, r, v) = parse_owner_repo("spraylee/tavily-mcp-multi-key").unwrap();
        assert_eq!((o, r, v), ("spraylee".into(), "tavily-mcp-multi-key".into(), None));
        let (o, r, v) = parse_owner_repo("BurntSushi/ripgrep@15.2.0").unwrap();
        assert_eq!(o, "BurntSushi");
        assert_eq!(r, "ripgrep");
        assert_eq!(v.as_deref(), Some("15.2.0"));
        assert!(parse_owner_repo("owner/repo@").is_err());
        assert!(parse_owner_repo("@scope/pkg").is_err());
    }
}

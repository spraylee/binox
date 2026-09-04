use crate::archive::{self, archive_kind};
use crate::error::{Error, Result};
use crate::github::{list_assets, parse_latest_location, probe_latest_tag};
use crate::http::Http;
use crate::locate::locate_binary;
use crate::match_asset::{match_release_asset, repo_name_normalized, MatchStrategy};
use crate::platform;
use crate::util::{
    lookup_sha256, parse_sha256sums, set_executable, sha256_file, sidecar_path, warn,
};
use std::fs;
use std::path::Path;
use std::process::Command;

const DEFAULT_SELFUPDATE_REPO: &str = "spraylee/binox";

fn selfupdate_disabled() -> bool {
    matches!(
        std::env::var("BINOX_NO_SELFUPDATE").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

fn selfupdate_repo() -> Result<(String, String)> {
    let spec = std::env::var("BINOX_SELFUPDATE_REPO")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SELFUPDATE_REPO.to_string());
    let (owner, repo, _) = crate::cli::parse_owner_repo(&spec)?;
    Ok((owner, repo))
}

/// 主流程：fork 完立即返回。后台失败只警告，不影响退出码。
pub fn spawn_background_self_update() {
    if selfupdate_disabled() {
        return;
    }
    #[cfg(unix)]
    {
        spawn_unix();
    }
    #[cfg(windows)]
    {
        spawn_windows();
    }
}

#[cfg(unix)]
fn spawn_unix() {
    unsafe {
        let pid = libc::fork();
        if pid != 0 {
            return;
        }
        libc::setsid();
        let pid2 = libc::fork();
        if pid2 != 0 {
            libc::_exit(0);
        }
        let devnull = libc::open(
            b"/dev/null\0".as_ptr() as *const libc::c_char,
            libc::O_RDWR,
        );
        if devnull >= 0 {
            libc::dup2(devnull, 0);
            libc::dup2(devnull, 1);
            if devnull > 2 {
                libc::close(devnull);
            }
        }
        libc::chdir(b"/\0".as_ptr() as *const libc::c_char);
    }
    if let Err(err) = run_self_update(false) {
        warn(format!("自更新失败: {err}"));
    }
    unsafe {
        libc::_exit(0);
    }
}

#[cfg(windows)]
fn spawn_windows() {
    if let Ok(exe) = std::env::current_exe() {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        let _ = Command::new(exe)
            .arg("--self-update-worker")
            .env("BINOX_NO_SELFUPDATE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn();
    }
}

pub fn run_foreground() -> Result<()> {
    run_self_update(true)
}

/// Windows / 测试入口：后台 worker。
pub fn run_worker() -> Result<()> {
    if let Err(err) = run_self_update(false) {
        warn(format!("自更新失败: {err}"));
    }
    Ok(())
}

fn run_self_update(foreground: bool) -> Result<()> {
    let http = Http::for_self_update()?;
    let platform = platform::detect()?;

    if let Some(url) = std::env::var("BINOX_SELFUPDATE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        return update_from_override_url(&http, url.trim(), &platform.triple, foreground);
    }

    let (owner, repo) = selfupdate_repo()?;
    let tag = probe_latest_tag(&http, &owner, &repo)?;
    let current = env!("CARGO_PKG_VERSION");
    if is_same_version(&tag, current) {
        if foreground {
            crate::util::log(format!("已是最新版本 {current}（Release {tag}）"));
        }
        return Ok(());
    }

    if foreground {
        crate::util::log(format!("发现新版本 {tag}，开始自更新"));
    }
    download_and_replace(&http, &owner, &repo, &tag, &platform.triple, foreground)
}

fn is_same_version(tag: &str, current: &str) -> bool {
    let tag_ver = crate::match_asset::ver_from_tag(tag);
    tag_ver == current || tag == current || tag == format!("v{current}")
}

fn update_from_override_url(
    http: &Http,
    url: &str,
    host_triple: &str,
    foreground: bool,
) -> Result<()> {
    // 覆盖 URL 当作 latest 探测；任何失败都变成警告（前台则返回 Err 给 run_foreground）。
    match http.probe_location(url) {
        Ok(Some(loc)) => {
            if let Some(tag) = parse_latest_location(&loc) {
                let (owner, repo) = selfupdate_repo()?;
                if is_same_version(&tag, env!("CARGO_PKG_VERSION")) {
                    if foreground {
                        crate::util::log(format!("已是最新版本（{tag}）"));
                    }
                    return Ok(());
                }
                return download_and_replace(http, &owner, &repo, &tag, host_triple, foreground);
            }
            return Err(Error::new(format!(
                "自更新探测 URL 未返回可解析 tag: {url} → {loc}"
            )));
        }
        Ok(None) => Err(Error::new(format!("自更新探测失败（无 Location）: {url}"))),
        Err(err) => Err(Error::new(format!("自更新探测失败: {err}"))),
    }
}

fn download_and_replace(
    http: &Http,
    owner: &str,
    repo: &str,
    tag: &str,
    host_triple: &str,
    foreground: bool,
) -> Result<()> {
    let assets = list_assets(http, owner, repo, tag)?;
    let matched = match_release_asset(&assets, repo, tag, host_triple, None)?;
    if matched.strategy == MatchStrategy::MuslFallback && foreground {
        crate::util::log(format!("自更新使用 musl 资产 {}", matched.name));
    }

    let tmp = std::env::temp_dir().join(format!(
        "binox-selfupdate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&tmp)?;
    let archive_path = tmp.join(&matched.name);
    let url = format!(
        "https://github.com/{owner}/{repo}/releases/download/{tag}/{}",
        matched.name
    );
    if foreground {
        crate::util::log(format!("下载 {url}"));
    }
    http.download_once_or_retry(&url, &archive_path)?;

    if assets.iter().any(|a| a == "SHA256SUMS") {
        let sums_path = tmp.join("SHA256SUMS");
        let sums_url =
            format!("https://github.com/{owner}/{repo}/releases/download/{tag}/SHA256SUMS");
        http.download_once_or_retry(&sums_url, &sums_path)?;
        let text = fs::read_to_string(&sums_path)?;
        let sums = parse_sha256sums(&text);
        let expected = lookup_sha256(&sums, &matched.name).ok_or_else(|| {
            Error::new(format!("SHA256SUMS 中没有 {} 的条目", matched.name))
        })?;
        let actual = sha256_file(&archive_path)?;
        if actual != expected {
            let _ = fs::remove_dir_all(&tmp);
            return Err(Error::new(format!(
                "自更新 SHA256 校验失败: {} expected={expected} actual={actual}",
                matched.name
            )));
        }
        if foreground {
            crate::util::log(format!("SHA256 校验通过: {}", matched.name));
        }
    } else if foreground {
        warn("自更新未找到 SHA256SUMS，跳过校验");
    }

    let extracted = tmp.join("extracted");
    archive::extract(&archive_path, &extracted, archive_kind(&matched.name))?;
    let hints = vec![repo_name_normalized(repo), "binox".into()];
    let new_bin = locate_binary(&extracted, &hints, Some("binox"), &format!("{owner}/{repo}"))?;
    replace_self(&new_bin, foreground)?;
    let _ = fs::remove_dir_all(&tmp);
    Ok(())
}

fn replace_self(new_bin: &Path, foreground: bool) -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| Error::new(format!("无法定位自身: {e}")))?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    let new_path = sidecar_path(&exe, ".new");
    let bak_path = sidecar_path(&exe, ".bak");

    fs::copy(new_bin, &new_path)?;
    set_executable(&new_path)?;
    if exe.exists() {
        fs::copy(&exe, &bak_path)?;
    }
    atomic_replace(&new_path, &exe)?;

    let smoke = Command::new(&exe)
        .arg("--version")
        .env("BINOX_NO_SELFUPDATE", "1")
        .output();
    let ok = match smoke {
        Ok(out) => out.status.success() && String::from_utf8_lossy(&out.stdout).contains("binox"),
        Err(_) => false,
    };
    if !ok {
        if bak_path.exists() {
            let _ = atomic_replace(&bak_path, &exe);
        }
        return Err(Error::new("自更新冒烟失败，已回滚"));
    }
    let _ = fs::remove_file(&bak_path);
    if foreground {
        crate::util::log(format!("自更新完成: {}", exe.display()));
    }
    Ok(())
}

fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(from, to)?;
            let _ = fs::remove_file(from);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_version_compares_tag_and_package() {
        assert!(is_same_version("v0.1.0", "0.1.0"));
        assert!(is_same_version("0.1.0", "0.1.0"));
        assert!(!is_same_version("v0.2.0", "0.1.0"));
    }
}

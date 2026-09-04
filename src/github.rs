use crate::archive::{self, archive_kind};
use crate::cache;
use crate::error::{Error, Result};
use crate::http::Http;
use crate::locate::locate_binary;
use crate::match_asset::{
    asset_name_prefix, match_release_asset, repo_name_normalized, MatchStrategy,
};
use crate::platform::Platform;
use crate::util::{lookup_sha256, parse_sha256sums, sha256_file, warn};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub fn prepare(
    http: &Http,
    owner: &str,
    repo: &str,
    version_spec: Option<&str>,
    bin: Option<&str>,
    asset_template: Option<&str>,
    offline: bool,
    platform: &Platform,
) -> Result<PathBuf> {
    let _lock = cache::acquire_lock(owner, repo)?;

    let tag = resolve_tag(http, owner, repo, version_spec, offline)?;
    for triple in platform.candidate_triples() {
        let dest = cache::version_dir(owner, repo, &tag, &triple);
        if let Some(cached) = cache::read_cached_bin(&dest) {
            cache::log_cache_hit(&tag, &triple);
            cache::write_last_version(owner, repo, &tag)?;
            return Ok(cached);
        }
    }

    if offline {
        return Err(Error::new(format!(
            "离线模式且 {owner}/{repo}@{tag} 没有可用缓存（平台 {}）",
            platform.triple
        )));
    }

    let assets = list_assets(http, owner, repo, &tag)?;
    let matched = match_release_asset(
        &assets,
        repo,
        &tag,
        &platform.triple,
        asset_template,
    )?;
    match matched.strategy {
        MatchStrategy::MuslFallback => {
            crate::util::log(format!(
                "gnu 资产未命中，回退 musl: {} ({})",
                matched.name, matched.target
            ));
        }
        MatchStrategy::Fuzzy => {
            warn(format!(
                "未命中标准资产矩阵，使用模糊匹配 {}",
                matched.name
            ));
        }
        MatchStrategy::Matrix | MatchStrategy::Template => {}
    }

    let dest = cache::version_dir(owner, repo, &tag, &matched.target);
    let work_root = dest
        .parent()
        .unwrap_or(&dest)
        .join(format!(".part-{}", std::process::id()));
    if work_root.exists() {
        fs::remove_dir_all(&work_root)?;
    }
    fs::create_dir_all(&work_root)?;

    let archive_path = work_root.join(&matched.name);
    let url = format!(
        "https://github.com/{owner}/{repo}/releases/download/{tag}/{}",
        matched.name
    );
    crate::util::log(format!("下载 {url}"));
    http.download(&url, &archive_path)?;

    if assets.iter().any(|a| a == "SHA256SUMS") {
        verify_github_sha256(http, owner, repo, &tag, &matched.name, &archive_path, &work_root)?;
    } else {
        warn("未找到 SHA256SUMS 资产，跳过校验");
    }

    let extracted = work_root.join("extracted");
    archive::extract(&archive_path, &extracted, archive_kind(&matched.name))?;

    let mut hints = vec![repo_name_normalized(repo)];
    let prefix = asset_name_prefix(&matched.name);
    if !prefix.is_empty() && !hints.iter().any(|h| h == &prefix) {
        hints.push(prefix);
    }
    let package = format!("{owner}/{repo}");
    let binary = locate_binary(&extracted, &hints, bin, &package)?;
    let rel = binary
        .strip_prefix(&extracted)
        .map(Path::to_path_buf)
        .unwrap_or(binary);

    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(dest.parent().unwrap_or(&dest))?;
    fs::rename(&extracted, &dest)?;
    let _ = fs::remove_dir_all(&work_root);

    let final_bin = dest.join(&rel);
    cache::write_cached_bin(&dest, &final_bin)?;
    cache::write_last_version(owner, repo, &tag)?;
    Ok(final_bin)
}

fn resolve_tag(
    http: &Http,
    owner: &str,
    repo: &str,
    spec: Option<&str>,
    offline: bool,
) -> Result<String> {
    if let Some(spec) = spec.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(spec.to_string());
    }
    if offline {
        return cache::read_last_version(owner, repo).ok_or_else(|| {
            Error::new(format!(
                "离线模式且 {owner}/{repo} 没有 last-version 缓存"
            ))
        });
    }

    match probe_latest(http, owner, repo) {
        Ok(tag) => {
            crate::util::log(format!("最新版本 {owner}/{repo}: {tag}"));
            Ok(tag)
        }
        Err(err) => {
            if let Some(last) = cache::read_last_version(owner, repo) {
                eprintln!("[binox] 探测失败，使用缓存版本 {last}");
                Ok(last)
            } else {
                Err(Error::new(format!(
                    "无法探测 {owner}/{repo} 的最新 Release，且无缓存版本: {err}"
                )))
            }
        }
    }
}

fn probe_latest(http: &Http, owner: &str, repo: &str) -> Result<String> {
    let url = format!("https://github.com/{owner}/{repo}/releases/latest");
    if let Some(loc) = http.probe_location(&url)? {
        if let Some(tag) = parse_latest_location(&loc) {
            return Ok(tag);
        }
    }
    let fallback =
        format!("https://github.com/{owner}/{repo}/releases/latest/download/SHA256SUMS");
    if let Some(loc) = http.probe_location(&fallback)? {
        if let Some(tag) = parse_latest_location(&loc) {
            return Ok(tag);
        }
    }
    Err(Error::new(format!(
        "GitHub /releases/latest 未返回可解析的 Location（{url}）"
    )))
}

/// 从 302 Location 取出 tag。
pub fn parse_latest_location(location: &str) -> Option<String> {
    if let Some(idx) = location.find("/releases/tag/") {
        let rest = &location[idx + "/releases/tag/".len()..];
        let tag = rest.split(['/', '?', '#']).next().filter(|s| !s.is_empty())?;
        return Some(tag.to_string());
    }
    if let Some(idx) = location.find("/releases/download/") {
        let rest = &location[idx + "/releases/download/".len()..];
        let tag = rest.split(['/', '?', '#']).next().filter(|s| !s.is_empty())?;
        return Some(tag.to_string());
    }
    None
}

pub fn list_assets(http: &Http, owner: &str, repo: &str, tag: &str) -> Result<Vec<String>> {
    let cache_file = cache::pkg_root(owner, repo).join(format!("assets-{tag}.txt"));
    if let Ok(text) = fs::read_to_string(&cache_file) {
        let names: Vec<String> = text
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !names.is_empty() {
            return Ok(names);
        }
    }

    let html_url = format!("https://github.com/{owner}/{repo}/releases/expanded_assets/{tag}");
    let names = match http.get_text(&html_url) {
        Ok(html) => parse_expanded_assets_html(&html, tag),
        Err(_) => Vec::new(),
    };
    let names = if names.is_empty() {
        crate::util::log("expanded_assets 不可用，回退 GitHub API（每小时匿名 60 次）");
        list_assets_api(http, owner, repo, tag)?
    } else {
        names
    };
    if names.is_empty() {
        return Err(Error::new(format!(
            "Release {owner}/{repo}@{tag} 没有可解析的资产"
        )));
    }
    let body = names.join("\n") + "\n";
    let _ = crate::util::atomic_write(&cache_file, body.as_bytes());
    Ok(names)
}

/// 从 expanded_assets HTML 抠 `/releases/download/{tag}/{name}`。
pub fn parse_expanded_assets_html(html: &str, tag: &str) -> Vec<String> {
    let needle = format!("/releases/download/{tag}/");
    let mut names = Vec::new();
    let mut rest = html;
    while let Some(idx) = rest.find(&needle) {
        let after = &rest[idx + needle.len()..];
        let end = after
            .find(|c: char| c == '"' || c == '\'' || c.is_whitespace() || c == '<' || c == '?')
            .unwrap_or(after.len());
        let raw = &after[..end];
        if !raw.is_empty() {
            let name = url_decode_minimal(raw);
            if !name.is_empty() && !names.iter().any(|n| n == &name) {
                names.push(name);
            }
        }
        rest = &after[end..];
    }
    names
}

fn url_decode_minimal(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn list_assets_api(http: &Http, owner: &str, repo: &str, tag: &str) -> Result<Vec<String>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}");
    let text = http.get_text(&url)?;
    let value: Value = serde_json::from_str(&text)?;
    let mut names = Vec::new();
    if let Some(assets) = value.get("assets").and_then(|v| v.as_array()) {
        for asset in assets {
            if let Some(name) = asset.get("name").and_then(|v| v.as_str()) {
                names.push(name.to_string());
            }
        }
    }
    Ok(names)
}

fn verify_github_sha256(
    http: &Http,
    owner: &str,
    repo: &str,
    tag: &str,
    asset_name: &str,
    archive_path: &Path,
    work_root: &Path,
) -> Result<()> {
    let sums_path = work_root.join("SHA256SUMS");
    let url = format!("https://github.com/{owner}/{repo}/releases/download/{tag}/SHA256SUMS");
    http.download(&url, &sums_path)?;
    let text = fs::read_to_string(&sums_path)?;
    let sums = parse_sha256sums(&text);
    let expected = lookup_sha256(&sums, asset_name).ok_or_else(|| {
        Error::new(format!("SHA256SUMS 中没有 {asset_name} 的条目"))
    })?;
    let actual = sha256_file(archive_path)?;
    if actual != expected {
        return Err(Error::new(format!(
            "SHA256 校验失败: {asset_name} expected={expected} actual={actual}"
        )));
    }
    crate::util::log(format!("SHA256 校验通过: {asset_name}"));
    Ok(())
}

/// 供自更新复用：探测 latest tag（不写主流程日志）。
pub fn probe_latest_tag(http: &Http, owner: &str, repo: &str) -> Result<String> {
    probe_latest(http, owner, repo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_latest_locations() {
        assert_eq!(
            parse_latest_location(
                "https://github.com/spraylee/tavily-mcp-multi-key/releases/tag/v0.4.1"
            )
            .as_deref(),
            Some("v0.4.1")
        );
        assert_eq!(
            parse_latest_location(
                "https://github.com/spraylee/tavily-mcp-multi-key/releases/download/v0.4.1/SHA256SUMS"
            )
            .as_deref(),
            Some("v0.4.1")
        );
        assert_eq!(
            parse_latest_location("https://github.com/BurntSushi/ripgrep/releases/tag/15.2.0")
                .as_deref(),
            Some("15.2.0")
        );
    }

    #[test]
    fn parse_expanded_assets_sample() {
        let html = r#"
            <a href="/spraylee/tavily-mcp-multi-key/releases/download/v0.4.1/SHA256SUMS">
            <a href="/spraylee/tavily-mcp-multi-key/releases/download/v0.4.1/tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz">
            <a href="/spraylee/tavily-mcp-multi-key/archive/refs/tags/v0.4.1.tar.gz">
        "#;
        let names = parse_expanded_assets_html(html, "v0.4.1");
        assert!(names.contains(&"SHA256SUMS".to_string()));
        assert!(names.contains(
            &"tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz".to_string()
        ));
        assert!(!names.iter().any(|n| n.contains("archive")));
    }
}

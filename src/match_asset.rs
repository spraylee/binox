use crate::error::{Error, Result};
use crate::platform::{musl_fallback_triple, target_aliases};
use crate::util::render_asset_template;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchStrategy {
    Template,
    Matrix,
    MuslFallback,
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMatch {
    pub name: String,
    pub target: String,
    pub strategy: MatchStrategy,
}

const ARCHIVE_SUFFIXES: &[&str] = &[".tar.gz", ".tgz", ".zip"];
const REJECT_SUBSTR: &[&str] = &[".deb", ".rpm", ".msi", ".sha", ".sig", ".json", ".txt"];

/// 仓库名小写，用作 `{name}`。
pub fn repo_name_normalized(repo: &str) -> String {
    repo.to_ascii_lowercase()
}

/// 从 tag 取矩阵用的 `{ver}`：去掉前导 `v`，再取末段 semver。
///
/// `v0.26.1` → `0.26.1`
/// `15.2.0` → `15.2.0`
/// `cargo-nextest-0.9.143` → `0.9.143`
pub fn ver_from_tag(tag: &str) -> String {
    let trimmed = tag.strip_prefix('v').unwrap_or(tag);
    if let Some(semver) = last_semver_segment(trimmed) {
        semver.to_string()
    } else {
        trimmed.to_string()
    }
}

/// 在字符串里找最后一段看起来像 semver 的子串（`N.N` / `N.N.N`，可带 -pre / +build）。
pub fn last_semver_segment(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut last = None;
    let mut i = 0;
    while i < bytes.len() {
        let at_boundary = i == 0 || matches!(bytes[i - 1], b'-' | b'_' | b'/');
        if bytes[i].is_ascii_digit() && at_boundary {
            let rest = &s[i..];
            let candidate = rest.split(['/', '\\']).next().unwrap_or(rest);
            if looks_like_semver(candidate) {
                last = Some(candidate);
            }
        }
        i += 1;
    }
    last
}

fn looks_like_semver(s: &str) -> bool {
    let core = s.split(['-', '+']).next().unwrap_or(s);
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() < 2 || parts.len() > 4 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// 资产矩阵文件名候选（按任务书顺序）。
pub fn matrix_candidates(name: &str, ver: &str, target: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: String| {
        if !out.iter().any(|e| e == &s) {
            out.push(s);
        }
    };
    for ext in [".tar.gz", ".tgz"] {
        push(format!("{name}-v{ver}-{target}{ext}"));
    }
    for ext in [".tar.gz", ".tgz"] {
        push(format!("{name}-{ver}-{target}{ext}"));
    }
    for ext in [".tar.gz", ".tgz"] {
        push(format!("{name}_{target}{ext}"));
    }
    for ext in [".tar.gz", ".tgz"] {
        push(format!("{name}-{target}{ext}"));
    }
    push(format!("{name}-{ver}-{target}.zip"));
    push(format!("{name}-v{ver}-{target}.zip"));
    out
}

fn first_present<'a>(assets: &'a [String], candidates: &[String]) -> Option<&'a str> {
    for cand in candidates {
        if let Some(found) = assets.iter().find(|a| *a == cand) {
            return Some(found.as_str());
        }
    }
    None
}

fn is_archive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ARCHIVE_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

fn is_rejected_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    REJECT_SUBSTR.iter().any(|s| lower.contains(s))
}

fn contains_alias(name: &str, aliases: &[String]) -> bool {
    let lower = name.to_ascii_lowercase();
    aliases.iter().any(|a| lower.contains(&a.to_ascii_lowercase()))
}

/// 模糊匹配：包含 triple 或其等价体，且是归档、不含发行包/校验文件后缀。
pub fn fuzzy_candidates(assets: &[String], triples: &[String]) -> Vec<String> {
    let mut aliases = Vec::new();
    for t in triples {
        for a in target_aliases(t) {
            if !aliases.iter().any(|e| e == &a) {
                aliases.push(a);
            }
        }
    }
    assets
        .iter()
        .filter(|name| {
            is_archive_name(name) && !is_rejected_name(name) && contains_alias(name, &aliases)
        })
        .cloned()
        .collect()
}

pub fn format_assets(assets: &[String]) -> String {
    if assets.is_empty() {
        "  （空）".into()
    } else {
        assets
            .iter()
            .map(|a| format!("  - {a}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 列举式匹配：模板覆盖 → 本机矩阵 → musl 矩阵 → 模糊。
pub fn match_release_asset(
    assets: &[String],
    repo: &str,
    tag: &str,
    host_triple: &str,
    template: Option<&str>,
) -> Result<AssetMatch> {
    let name = repo_name_normalized(repo);
    if let Some(tpl) = template.map(str::trim).filter(|s| !s.is_empty()) {
        let wanted = render_asset_template(tpl, &name, tag, host_triple);
        if assets.iter().any(|a| a == &wanted) {
            return Ok(AssetMatch {
                name: wanted,
                target: host_triple.to_string(),
                strategy: MatchStrategy::Template,
            });
        }
        return Err(Error::new(format!(
            "GitHub 资产未匹配模板结果 `{wanted}`。\n实际资产：\n{}\n请调整 --asset-template（占位符 {{name}} {{version}} {{target}} {{tag}}）",
            format_assets(assets)
        )));
    }

    let ver = ver_from_tag(tag);
    let host_cands = matrix_candidates(&name, &ver, host_triple);
    if let Some(found) = first_present(assets, &host_cands) {
        return Ok(AssetMatch {
            name: found.to_string(),
            target: host_triple.to_string(),
            strategy: MatchStrategy::Matrix,
        });
    }

    if let Some(musl) = musl_fallback_triple(host_triple) {
        let musl_cands = matrix_candidates(&name, &ver, &musl);
        if let Some(found) = first_present(assets, &musl_cands) {
            return Ok(AssetMatch {
                name: found.to_string(),
                target: musl,
                strategy: MatchStrategy::MuslFallback,
            });
        }
    }

    let mut triples = vec![host_triple.to_string()];
    if let Some(musl) = musl_fallback_triple(host_triple) {
        triples.push(musl);
    }
    let fuzzy = fuzzy_candidates(assets, &triples);
    match fuzzy.len() {
        0 => Err(Error::new(format!(
            "GitHub 资产未匹配当前平台 {host_triple}（含 musl 回退）。\n实际资产：\n{}\n请用 --asset-template 覆盖（占位符 {{name}} {{version}} {{target}}）",
            format_assets(assets)
        ))),
        1 => Ok(AssetMatch {
            name: fuzzy[0].clone(),
            target: infer_target_from_asset(&fuzzy[0], &triples).unwrap_or_else(|| host_triple.to_string()),
            strategy: MatchStrategy::Fuzzy,
        }),
        _ => Err(Error::new(format!(
            "多个资产模糊匹配当前平台，请用 --asset-template 指定：\n{}",
            format_assets(&fuzzy)
        ))),
    }
}

fn infer_target_from_asset(asset: &str, triples: &[String]) -> Option<String> {
    triples.iter().find(|t| asset.contains(t.as_str())).cloned()
}

/// 资产名前缀：去掉归档后缀后，取第一个 `-` 或 `_` 之前。
pub fn asset_name_prefix(asset: &str) -> String {
    let mut base = asset;
    let lower = asset.to_ascii_lowercase();
    for suf in [".tar.gz", ".tgz", ".zip"] {
        if lower.ends_with(suf) {
            base = &asset[..asset.len() - suf.len()];
            break;
        }
    }
    base.split(['-', '_'])
        .next()
        .unwrap_or(base)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn ver_strips_leading_v_and_prefix() {
        assert_eq!(ver_from_tag("v0.26.1"), "0.26.1");
        assert_eq!(ver_from_tag("15.2.0"), "15.2.0");
        assert_eq!(ver_from_tag("0.14.9"), "0.14.9");
        assert_eq!(ver_from_tag("cargo-nextest-0.9.143"), "0.9.143");
        assert_eq!(ver_from_tag("jq-1.8.2"), "1.8.2");
        assert_eq!(ver_from_tag("v1.20.0-beta.1"), "1.20.0-beta.1");
    }

    #[test]
    fn matrix_order_covers_abc_and_zip() {
        let c = matrix_candidates("bat", "0.26.1", "x86_64-unknown-linux-gnu");
        assert_eq!(c[0], "bat-v0.26.1-x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(c[1], "bat-v0.26.1-x86_64-unknown-linux-gnu.tgz");
        assert_eq!(c[2], "bat-0.26.1-x86_64-unknown-linux-gnu.tar.gz");
        assert!(c.contains(&"bat_x86_64-unknown-linux-gnu.tar.gz".into()));
        assert!(c.contains(&"bat-x86_64-unknown-linux-gnu.tar.gz".into()));
        assert!(c.contains(&"bat-0.26.1-x86_64-unknown-linux-gnu.zip".into()));
        assert!(c.contains(&"bat-v0.26.1-x86_64-unknown-linux-gnu.zip".into()));
    }

    #[test]
    fn match_bat_class_b() {
        let assets = names(&[
            "bat-v0.26.1-x86_64-unknown-linux-gnu.tar.gz",
            "bat-v0.26.1-aarch64-apple-darwin.tar.gz",
            "SHA256SUMS",
        ]);
        let m = match_release_asset(
            &assets,
            "bat",
            "v0.26.1",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(m.name, "bat-v0.26.1-x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(m.strategy, MatchStrategy::Matrix);
        assert_eq!(m.target, "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn match_ripgrep_class_a_musl_fallback() {
        let assets = names(&[
            "ripgrep-15.2.0-aarch64-apple-darwin.tar.gz",
            "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz",
            "ripgrep_15.2.0-1_amd64.deb",
        ]);
        let m = match_release_asset(
            &assets,
            "ripgrep",
            "15.2.0",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(m.name, "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz");
        assert_eq!(m.strategy, MatchStrategy::MuslFallback);
        assert_eq!(m.target, "x86_64-unknown-linux-musl");
    }

    #[test]
    fn match_zoxide_musl_and_eza_underscore() {
        let zoxide = match_release_asset(
            &names(&["zoxide-0.10.0-x86_64-unknown-linux-musl.tar.gz"]),
            "zoxide",
            "v0.10.0",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(zoxide.strategy, MatchStrategy::MuslFallback);

        let eza = match_release_asset(
            &names(&["eza_x86_64-unknown-linux-gnu.tar.gz"]),
            "eza",
            "v0.23.5",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(eza.name, "eza_x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(eza.strategy, MatchStrategy::Matrix);
    }

    #[test]
    fn match_starship_no_version_and_bottom_underscore() {
        let starship = match_release_asset(
            &names(&["starship-x86_64-unknown-linux-gnu.tar.gz"]),
            "starship",
            "v1.26.0",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(starship.name, "starship-x86_64-unknown-linux-gnu.tar.gz");

        let bottom = match_release_asset(
            &names(&["bottom_x86_64-unknown-linux-gnu.tar.gz"]),
            "bottom",
            "0.14.9",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(bottom.name, "bottom_x86_64-unknown-linux-gnu.tar.gz");
    }

    #[test]
    fn match_just_musl_and_binstall_tgz() {
        let just = match_release_asset(
            &names(&["just-1.58.0-x86_64-unknown-linux-musl.tar.gz"]),
            "just",
            "1.58.0",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(just.strategy, MatchStrategy::MuslFallback);

        let binstall = match_release_asset(
            &names(&["cargo-binstall-x86_64-unknown-linux-gnu.tgz"]),
            "cargo-binstall",
            "v1.22.0",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(binstall.name, "cargo-binstall-x86_64-unknown-linux-gnu.tgz");
        assert_eq!(binstall.strategy, MatchStrategy::Matrix);
    }

    #[test]
    fn match_zip_variant() {
        let m = match_release_asset(
            &names(&["fd-v10.5.0-x86_64-pc-windows-msvc.zip"]),
            "fd",
            "v10.5.0",
            "x86_64-pc-windows-msvc",
            None,
        )
        .unwrap();
        assert_eq!(m.name, "fd-v10.5.0-x86_64-pc-windows-msvc.zip");
        assert_eq!(m.strategy, MatchStrategy::Matrix);
    }

    #[test]
    fn tag_prefix_then_fuzzy_unique() {
        let assets = names(&[
            "cargo-nextest-0.9.143-x86_64-unknown-linux-gnu.tar.gz",
            "cargo-nextest-0.9.143-aarch64-apple-darwin.tar.gz",
        ]);
        let m = match_release_asset(
            &assets,
            "nextest",
            "cargo-nextest-0.9.143",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(
            m.name,
            "cargo-nextest-0.9.143-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(m.strategy, MatchStrategy::Fuzzy);
    }

    #[test]
    fn fuzzy_rejects_deb_and_checksums() {
        let assets = names(&[
            "ripgrep_15.2.0-1_amd64.deb",
            "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz.sha256",
            "notes.txt",
        ]);
        let err = match_release_asset(
            &assets,
            "ripgrep",
            "15.2.0",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap_err();
        assert!(err.message.contains("未匹配"));
    }

    #[test]
    fn fuzzy_multiple_asks_for_template() {
        let assets = names(&[
            "foo-x86_64-unknown-linux-gnu.tar.gz",
            "bar-x86_64-unknown-linux-gnu.tar.gz",
        ]);
        let err = match_release_asset(
            &assets,
            "unknown-tool",
            "v1.0.0",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap_err();
        assert!(err.message.contains("--asset-template"));
        assert!(err.message.contains("foo-"));
        assert!(err.message.contains("bar-"));
    }

    #[test]
    fn template_overrides_everything() {
        let assets = names(&[
            "custom-name.tar.gz",
            "bat-v0.26.1-x86_64-unknown-linux-gnu.tar.gz",
        ]);
        let m = match_release_asset(
            &assets,
            "bat",
            "v0.26.1",
            "x86_64-unknown-linux-gnu",
            Some("custom-name.tar.gz"),
        )
        .unwrap();
        assert_eq!(m.name, "custom-name.tar.gz");
        assert_eq!(m.strategy, MatchStrategy::Template);

        let err = match_release_asset(
            &assets,
            "bat",
            "v0.26.1",
            "x86_64-unknown-linux-gnu",
            Some("{name}-nope-{target}.tar.gz"),
        )
        .unwrap_err();
        assert!(err.message.contains("未匹配模板"));
    }

    #[test]
    fn asset_prefix_from_hyphen_and_underscore() {
        assert_eq!(
            asset_name_prefix("bat-v0.26.1-x86_64-unknown-linux-gnu.tar.gz"),
            "bat"
        );
        assert_eq!(
            asset_name_prefix("eza_x86_64-unknown-linux-gnu.tar.gz"),
            "eza"
        );
        assert_eq!(
            asset_name_prefix("bottom_x86_64-unknown-linux-gnu.tar.gz"),
            "bottom"
        );
        assert_eq!(
            asset_name_prefix("tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz"),
            "tavily"
        );
    }

    #[test]
    fn tavily_class_b() {
        let m = match_release_asset(
            &names(&["tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz"]),
            "tavily-mcp-multi-key",
            "v0.4.1",
            "x86_64-unknown-linux-gnu",
            None,
        )
        .unwrap();
        assert_eq!(
            m.name,
            "tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(m.strategy, MatchStrategy::Matrix);
    }
}

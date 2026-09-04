use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// `{version}` = 原始 tag 去掉可选的前导 `v`（不剥 crate 前缀）。
pub fn version_without_v(version: &str) -> &str {
    version.strip_prefix('v').unwrap_or(version)
}

/// 渲染 `--asset-template`。
///
/// 占位符语义：
/// - `{name}`：仓库名小写
/// - `{version}`：原始 tag 去掉前导 `v`（`v1.2.3` → `1.2.3`；`15.2.0` 保持；`cargo-nextest-0.9.143` 保持全文）
/// - `{target}`：平台 triple
/// - `{tag}`：原始 tag 全文
pub fn render_asset_template(template: &str, name: &str, tag: &str, target: &str) -> String {
    template
        .replace("{name}", name)
        .replace("{version}", version_without_v(tag))
        .replace("{target}", target)
        .replace("{tag}", tag)
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// 解析 SHA256SUMS：`hash  name` 或 `hash *name`。
pub fn parse_sha256sums(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        let Some(mut name) = parts.next() else { continue };
        if let Some(stripped) = name.strip_prefix('*') {
            name = stripped;
        }
        if let Some(stripped) = name.strip_prefix("./") {
            name = stripped;
        }
        out.push((hash.to_ascii_lowercase(), name.to_string()));
    }
    out
}

pub fn lookup_sha256(sums: &[(String, String)], filename: &str) -> Option<String> {
    let base = filename.rsplit('/').next().unwrap_or(filename);
    sums.iter().find_map(|(hash, name)| {
        let name_base = name.rsplit('/').next().unwrap_or(name);
        if name == filename || name_base == base || name_base == filename {
            Some(hash.clone())
        } else {
            None
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Elf,
    MachO,
    Pe,
    Other,
}

impl FileKind {
    pub fn is_native(self) -> bool {
        matches!(self, Self::Elf | Self::MachO | Self::Pe)
    }
}

pub fn classify_file(path: &Path) -> Result<FileKind> {
    let mut file = File::open(path)?;
    let mut buf = [0u8; 256];
    let n = file.read(&mut buf)?;
    Ok(classify_bytes(&buf[..n]))
}

pub fn classify_bytes(buf: &[u8]) -> FileKind {
    if buf.len() >= 4 && buf.starts_with(b"\x7fELF") {
        return FileKind::Elf;
    }
    if buf.len() >= 2 && buf.starts_with(b"MZ") {
        return FileKind::Pe;
    }
    if buf.len() >= 4 {
        let magic = [buf[0], buf[1], buf[2], buf[3]];
        if matches!(
            magic,
            [0xfe, 0xed, 0xfa, 0xce]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xce, 0xfa, 0xed, 0xfe]
                | [0xcf, 0xfa, 0xed, 0xfe]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [0xbe, 0xba, 0xfe, 0xca]
        ) {
            return FileKind::MachO;
        }
    }
    FileKind::Other
}

pub fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk_files_inner(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_files_inner(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            walk_files_inner(&path, out)?;
        } else if meta.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

pub fn file_name_eq(path: &Path, wanted: &str) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|n| n == wanted || n == format!("{wanted}.exe"))
}

pub fn safe_join(base: &Path, rel: &Path) -> Result<PathBuf> {
    if rel.is_absolute() {
        return Err(Error::new(format!("归档内含绝对路径: {}", rel.display())));
    }
    for c in rel.components() {
        if matches!(c, Component::ParentDir) {
            return Err(Error::new(format!("归档内含非法路径: {}", rel.display())));
        }
    }
    Ok(base.join(rel))
}

#[cfg(unix)]
pub fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("part");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

pub fn log(msg: impl std::fmt::Display) {
    eprintln!("[binox] {msg}");
}

pub fn warn(msg: impl std::fmt::Display) {
    eprintln!("[binox] 警告: {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_template_version_is_raw_tag_minus_leading_v() {
        let rendered = render_asset_template(
            "{name}-v{version}-{target}.tar.gz",
            "tavily-mcp-multi-key",
            "v0.4.1",
            "x86_64-unknown-linux-gnu",
        );
        assert_eq!(
            rendered,
            "tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz"
        );
        let no_v = render_asset_template(
            "{name}-{version}-{target}.tar.gz",
            "ripgrep",
            "15.2.0",
            "x86_64-unknown-linux-musl",
        );
        assert_eq!(no_v, "ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz");
        let prefixed = render_asset_template(
            "{name}-{version}-{target}.tar.gz",
            "nextest",
            "cargo-nextest-0.9.143",
            "x86_64-unknown-linux-gnu",
        );
        assert_eq!(
            prefixed,
            "nextest-cargo-nextest-0.9.143-x86_64-unknown-linux-gnu.tar.gz"
        );
        let with_tag = render_asset_template("{tag}", "x", "v1.2.3", "t");
        assert_eq!(with_tag, "v1.2.3");
    }

    #[test]
    fn classify_elf_macho_pe() {
        assert_eq!(classify_bytes(b"\x7fELF\x02\x01"), FileKind::Elf);
        assert_eq!(classify_bytes(b"MZ\x90\x00"), FileKind::Pe);
        assert_eq!(
            classify_bytes(&[0xfe, 0xed, 0xfa, 0xcf, 0x00]),
            FileKind::MachO
        );
        assert_eq!(classify_bytes(b"#!/usr/bin/env node\n"), FileKind::Other);
    }

    #[test]
    fn sha256sums_parse() {
        let text = "\
8fffbc37af1ee83fa0940f82cc3a1f1600046fe0b6b18b8b159c88be53812a32  bootstrap.sh
535190e60fb4bd9afc1d8d24e2edbc0f90a19a49e0366836b87aaea538dd0154  tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz
";
        let sums = parse_sha256sums(text);
        assert_eq!(
            lookup_sha256(
                &sums,
                "tavily-mcp-multi-key-v0.4.1-x86_64-unknown-linux-gnu.tar.gz"
            )
            .as_deref(),
            Some("535190e60fb4bd9afc1d8d24e2edbc0f90a19a49e0366836b87aaea538dd0154")
        );
    }
}

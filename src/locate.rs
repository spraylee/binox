use crate::error::{Error, Result};
use crate::util::{classify_file, file_name_eq, set_executable, walk_files, FileKind};
use std::path::{Path, PathBuf};

fn skip_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if name.starts_with('.') {
        return true;
    }
    if lower.ends_with(".sha")
        || lower.ends_with(".txt")
        || lower.ends_with(".sha256")
        || lower.ends_with(".sig")
        || lower.ends_with(".md")
        || lower.ends_with(".json")
    {
        return true;
    }
    matches!(
        name,
        "SHA256SUMS" | "LICENSE" | "LICENCE" | "CHANGELOG.md" | "README.md" | ".binox-bin"
    )
}

fn push_unique_path(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !out.iter().any(|p| p == &path) {
        out.push(path);
    }
}

/// 在解压目录里定位原生二进制。
///
/// 1. `--bin` 指定名（无扩展名，Windows 可带 .exe）
/// 2. 候选名 = 仓库名小写 + 资产名前缀，在根目录与子目录递归匹配 ELF/Mach-O/PE
/// 3. 命中多个：列出并提示 --bin；恰好一个：直接用
/// 4. 候选名全 miss 且恰好一个原生文件（bottom=`btm`、ripgrep=`rg`）：用它
pub fn locate_binary(
    root: &Path,
    hinted_names: &[String],
    explicit_bin: Option<&str>,
    package_label: &str,
) -> Result<PathBuf> {
    let files = walk_files(root)?;
    let mut classified = Vec::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if skip_name(name) {
            continue;
        }
        if let Ok(kind) = classify_file(&path) {
            classified.push((path, kind));
        }
    }

    if let Some(name) = explicit_bin {
        let matches: Vec<&(PathBuf, FileKind)> = classified
            .iter()
            .filter(|(p, _)| file_name_eq(p, name))
            .collect();
        if matches.is_empty() {
            return Err(not_found(root, &classified, &format!("--bin {name}")));
        }
        if let Some((path, _)) = matches.iter().copied().find(|(_, k)| k.is_native()) {
            set_executable(path)?;
            return Ok(path.clone());
        }
        let path = &matches[0].0;
        set_executable(path)?;
        return Ok(path.clone());
    }

    let mut hint_hits = Vec::new();
    for hint in hinted_names {
        for (path, kind) in &classified {
            if kind.is_native() && file_name_eq(path, hint) {
                push_unique_path(&mut hint_hits, path.clone());
            }
        }
    }
    if hint_hits.len() == 1 {
        set_executable(&hint_hits[0])?;
        return Ok(hint_hits[0].clone());
    }
    if hint_hits.len() > 1 {
        return Err(not_found(root, &classified, "多个原生二进制匹配候选名"));
    }

    let natives: Vec<&(PathBuf, FileKind)> = classified
        .iter()
        .filter(|(_, k)| k.is_native())
        .collect();
    if natives.len() == 1 {
        let path = &natives[0].0;
        set_executable(path)?;
        return Ok(path.clone());
    }
    if natives.len() > 1 {
        return Err(not_found(
            root,
            &classified,
            &format!("解压后有多个原生二进制（{package_label}）"),
        ));
    }
    Err(not_found(root, &classified, "未找到可执行文件"))
}

fn not_found(root: &Path, classified: &[(PathBuf, FileKind)], why: &str) -> Error {
    let mut listing = String::new();
    for (path, kind) in classified {
        let rel = path.strip_prefix(root).unwrap_or(path);
        listing.push_str(&format!("  {} ({kind:?})\n", rel.display()));
    }
    if listing.is_empty() {
        listing.push_str("  （目录为空）\n");
    }
    Error::new(format!(
        "{why}。解压目录 {} 内的文件：\n{listing}请用 --bin <name> 指定",
        root.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("binox-locate-{nanos}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_elf(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"\x7fELF\x02\x01\x01 fake-elf").unwrap();
    }

    fn write_text(path: &Path, body: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn explicit_bin_wins() {
        let root = temp_root();
        write_elf(&root.join("rg"));
        write_elf(&root.join("other"));
        let found = locate_binary(&root, &["ripgrep".into()], Some("rg"), "BurntSushi/ripgrep")
            .unwrap();
        assert_eq!(found.file_name().unwrap(), "rg");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn hint_repo_name_in_nested_dir() {
        let root = temp_root();
        write_elf(&root.join("bat-v0.26.1-x86_64-unknown-linux-gnu").join("bat"));
        write_text(&root.join("README.txt"), b"docs");
        let found = locate_binary(&root, &["bat".into(), "bat".into()], None, "sharkdp/bat").unwrap();
        assert_eq!(found.file_name().unwrap(), "bat");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn single_native_when_hints_miss_rg_and_btm() {
        let root = temp_root();
        write_elf(&root.join("ripgrep-15.2.0-x86_64-unknown-linux-musl").join("rg"));
        write_text(
            &root
                .join("ripgrep-15.2.0-x86_64-unknown-linux-musl")
                .join("README.md"),
            b"md",
        );
        let found = locate_binary(
            &root,
            &["ripgrep".into(), "ripgrep".into()],
            None,
            "BurntSushi/ripgrep",
        )
        .unwrap();
        assert_eq!(found.file_name().unwrap(), "rg");
        let _ = fs::remove_dir_all(&root);

        let root = temp_root();
        write_elf(&root.join("btm"));
        write_text(&root.join("CHANGELOG.txt"), b"notes");
        let found = locate_binary(
            &root,
            &["bottom".into(), "bottom".into()],
            None,
            "ClementTsang/bottom",
        )
        .unwrap();
        assert_eq!(found.file_name().unwrap(), "btm");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn skips_sha_and_txt() {
        let root = temp_root();
        write_elf(&root.join("tool.sha"));
        write_text(&root.join("notes.txt"), b"\x7fELF would-be");
        write_elf(&root.join("realbin"));
        let found = locate_binary(&root, &["nope".into()], None, "owner/repo").unwrap();
        assert_eq!(found.file_name().unwrap(), "realbin");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn multiple_natives_require_bin() {
        let root = temp_root();
        write_elf(&root.join("aaa"));
        write_elf(&root.join("bbb"));
        let err = locate_binary(&root, &["zzz".into()], None, "owner/repo").unwrap_err();
        assert!(err.message.contains("--bin"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn multiple_hint_hits_require_bin() {
        let root = temp_root();
        write_elf(&root.join("foo"));
        write_elf(&root.join("bar"));
        let err = locate_binary(
            &root,
            &["foo".into(), "bar".into()],
            None,
            "owner/repo",
        )
        .unwrap_err();
        assert!(err.message.contains("多个原生二进制匹配候选名"));
        let _ = fs::remove_dir_all(&root);
    }
}

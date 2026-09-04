use crate::error::Result;
use crate::util::{atomic_write, log};
use fs2::FileExt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// 缓存根：`$BINOX_CACHE_DIR` 或平台默认目录。
pub fn cache_root() -> PathBuf {
    if let Ok(dir) = std::env::var("BINOX_CACHE_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Caches/binox")
    } else if cfg!(windows) {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local).join("binox")
        } else {
            PathBuf::from(home).join("AppData/Local/binox")
        }
    } else if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg).join("binox")
    } else {
        PathBuf::from(home).join(".cache/binox")
    }
}

pub fn pkg_root(owner: &str, repo: &str) -> PathBuf {
    cache_root().join(owner).join(repo)
}

pub fn version_dir(owner: &str, repo: &str, version: &str, triple: &str) -> PathBuf {
    pkg_root(owner, repo).join(version).join(triple)
}

pub fn last_version_path(owner: &str, repo: &str) -> PathBuf {
    pkg_root(owner, repo).join("last-version")
}

pub fn read_last_version(owner: &str, repo: &str) -> Option<String> {
    let text = fs::read_to_string(last_version_path(owner, repo)).ok()?;
    let v = text.lines().next().unwrap_or("").trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

pub fn write_last_version(owner: &str, repo: &str, version: &str) -> Result<()> {
    atomic_write(
        &last_version_path(owner, repo),
        format!("{version}\n").as_bytes(),
    )
}

pub fn bin_sidecar(dir: &Path) -> PathBuf {
    dir.join(".binox-bin")
}

pub fn read_cached_bin(dir: &Path) -> Option<PathBuf> {
    let rel = fs::read_to_string(bin_sidecar(dir)).ok()?;
    let rel = rel.lines().next().unwrap_or("").trim();
    if rel.is_empty() {
        return None;
    }
    let path = dir.join(rel);
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

pub fn write_cached_bin(dir: &Path, binary: &Path) -> Result<()> {
    let rel = binary.strip_prefix(dir).unwrap_or(binary);
    atomic_write(&bin_sidecar(dir), format!("{}\n", rel.display()).as_bytes())
}

/// 目录锁：下载/解压期间独占，避免并发写坏缓存。drop 时解锁。
pub struct HeldLock {
    _file: File,
}

pub fn acquire_lock(owner: &str, repo: &str) -> Result<HeldLock> {
    let lock_path = pkg_root(owner, repo).join(".lock");
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(&lock_path)?;
    file.lock_exclusive()?;
    Ok(HeldLock { _file: file })
}

pub fn log_cache_hit(version: &str, triple: &str) {
    log(format!("缓存命中 0 下载: {version} ({triple})"));
}

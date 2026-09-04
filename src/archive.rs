use crate::error::{Error, Result};
use crate::util::{safe_join, set_executable};
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    TarGz,
    Zip,
    Raw,
}

pub fn archive_kind(name: &str) -> ArchiveKind {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        ArchiveKind::TarGz
    } else if lower.ends_with(".zip") {
        ArchiveKind::Zip
    } else {
        ArchiveKind::Raw
    }
}

pub fn extract(archive: &Path, dest: &Path, kind: ArchiveKind) -> Result<()> {
    fs::create_dir_all(dest)?;
    match kind {
        ArchiveKind::TarGz => extract_tar_gz(archive, dest),
        ArchiveKind::Zip => extract_zip(archive, dest),
        ArchiveKind::Raw => {
            let name = archive
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| Error::new("无法确定原始资产文件名"))?;
            let dest_file = dest.join(name);
            fs::copy(archive, &dest_file)?;
            set_executable(&dest_file)?;
            Ok(())
        }
    }
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let decoder = GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries()? {
        let mut entry = entry.map_err(|e| Error::new(format!("tar 条目损坏: {e}")))?;
        let rel = entry
            .path()
            .map_err(|e| Error::new(format!("tar 路径无效: {e}")))?
            .into_owned();
        let out = safe_join(dest, &rel)?;
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        entry
            .unpack(&out)
            .map_err(|e| Error::new(format!("解压 {} 失败: {e}", rel.display())))?;
    }
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut zf = zip.by_index(i)?;
        let Some(rel) = zf.enclosed_name().map(PathBuf::from) else {
            continue;
        };
        let out = safe_join(dest, &rel)?;
        if zf.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut outfile = File::create(&out)?;
        io::copy(&mut zf, &mut outfile)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = zf.unix_mode() {
                fs::set_permissions(&out, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_archive_kinds() {
        assert_eq!(archive_kind("foo.tar.gz"), ArchiveKind::TarGz);
        assert_eq!(archive_kind("foo.tgz"), ArchiveKind::TarGz);
        assert_eq!(archive_kind("foo.ZIP"), ArchiveKind::Zip);
        assert_eq!(archive_kind("foo"), ArchiveKind::Raw);
    }
}

use crate::error::{Error, Result};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Libc {
    Gnu,
    Musl,
    None,
}

/// 当前平台：Rust target triple。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    pub triple: String,
    pub libc: Libc,
}

/// 从 `ldd --version` 的输出判断是否 musl（纯函数，供单测）。
pub fn musl_from_ldd_output(output: &str) -> bool {
    output.to_ascii_lowercase().contains("musl")
}

/// 探测本机 libc：跑 `ldd --version`，失败则按 gnu 处理（macOS/Windows 不会走到这里）。
pub fn detect_musl() -> bool {
    let output = Command::new("ldd").arg("--version").output();
    match output {
        Ok(out) => {
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&out.stdout));
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            musl_from_ldd_output(&text)
        }
        Err(_) => false,
    }
}

/// 把 (os, arch, musl) 映射成 Rust target triple。
pub fn platform_from(os: &str, arch: &str, musl: bool) -> Result<Platform> {
    let rust_os = match os {
        "linux" => "unknown-linux",
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        other => {
            return Err(Error::new(format!(
                "不支持的操作系统 {other}（仅 linux / macos / windows）"
            )))
        }
    };
    let rust_arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        other => {
            return Err(Error::new(format!(
                "不支持的 CPU 架构 {other}（仅 x86_64 / aarch64）"
            )))
        }
    };

    let (triple, libc) = match (os, musl) {
        ("linux", true) => (format!("{rust_arch}-{rust_os}-musl"), Libc::Musl),
        ("linux", false) => (format!("{rust_arch}-{rust_os}-gnu"), Libc::Gnu),
        ("macos", _) => (format!("{rust_arch}-{rust_os}"), Libc::None),
        ("windows", _) => (format!("{rust_arch}-{rust_os}"), Libc::None),
        _ => unreachable!(),
    };

    Ok(Platform { triple, libc })
}

pub fn detect() -> Result<Platform> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let musl = os == "linux" && detect_musl();
    platform_from(os, arch, musl)
}

/// gnu 主机上的 musl 回退 triple。已是 musl / 非 Linux 则 None。
pub fn musl_fallback_triple(triple: &str) -> Option<String> {
    triple
        .strip_suffix("-gnu")
        .map(|prefix| format!("{prefix}-musl"))
}

impl Platform {
    /// 先本机 triple，gnu 再跟 musl。
    pub fn candidate_triples(&self) -> Vec<String> {
        let mut out = vec![self.triple.clone()];
        if let Some(musl) = musl_fallback_triple(&self.triple) {
            out.push(musl);
        }
        out
    }
}

/// 模糊匹配用的 triple 等价体（小写比较）。
pub fn target_aliases(triple: &str) -> Vec<String> {
    let mut out = vec![triple.to_string()];
    match triple {
        "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl" => {
            out.extend([
                "x86_64-linux".into(),
                "amd64-linux".into(),
                "linux-amd64".into(),
                "linux-x86_64".into(),
                "x86_64-unknown-linux".into(),
            ]);
        }
        "aarch64-unknown-linux-gnu" | "aarch64-unknown-linux-musl" => {
            out.extend([
                "aarch64-linux".into(),
                "arm64-linux".into(),
                "linux-arm64".into(),
                "linux-aarch64".into(),
                "aarch64-unknown-linux".into(),
            ]);
        }
        "x86_64-apple-darwin" => {
            out.extend([
                "x86_64-darwin".into(),
                "darwin-x86_64".into(),
                "macos-x86_64".into(),
            ]);
        }
        "aarch64-apple-darwin" => {
            out.extend([
                "aarch64-darwin".into(),
                "darwin-arm64".into(),
                "macos-arm64".into(),
            ]);
        }
        "x86_64-pc-windows-msvc" => {
            out.extend([
                "x86_64-windows".into(),
                "windows-amd64".into(),
                "pc-windows-msvc".into(),
            ]);
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musl_detect_from_ldd_text() {
        assert!(musl_from_ldd_output("musl libc (x86_64)\nVersion 1.2.5"));
        assert!(!musl_from_ldd_output(
            "ldd (Ubuntu GLIBC 2.39-0ubuntu8.7) 2.39\nCopyright (C) 2024"
        ));
    }

    #[test]
    fn linux_x64_gnu_triple() {
        let p = platform_from("linux", "x86_64", false).unwrap();
        assert_eq!(p.triple, "x86_64-unknown-linux-gnu");
        assert_eq!(p.libc, Libc::Gnu);
        assert_eq!(
            p.candidate_triples(),
            vec![
                "x86_64-unknown-linux-gnu".to_string(),
                "x86_64-unknown-linux-musl".to_string()
            ]
        );
    }

    #[test]
    fn linux_x64_musl_triple() {
        let p = platform_from("linux", "x86_64", true).unwrap();
        assert_eq!(p.triple, "x86_64-unknown-linux-musl");
        assert_eq!(p.libc, Libc::Musl);
        assert_eq!(
            p.candidate_triples(),
            vec!["x86_64-unknown-linux-musl".to_string()]
        );
        assert_eq!(musl_fallback_triple(&p.triple), None);
    }

    #[test]
    fn darwin_arm64_triple() {
        let p = platform_from("macos", "aarch64", false).unwrap();
        assert_eq!(p.triple, "aarch64-apple-darwin");
        assert_eq!(musl_fallback_triple(&p.triple), None);
    }

    #[test]
    fn windows_x64_triple() {
        let p = platform_from("windows", "x86_64", false).unwrap();
        assert_eq!(p.triple, "x86_64-pc-windows-msvc");
    }

    #[test]
    fn amd64_alias_maps_to_x86_64() {
        let p = platform_from("linux", "amd64", false).unwrap();
        assert_eq!(p.triple, "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn musl_fallback_only_from_gnu() {
        assert_eq!(
            musl_fallback_triple("x86_64-unknown-linux-gnu").as_deref(),
            Some("x86_64-unknown-linux-musl")
        );
        assert_eq!(
            musl_fallback_triple("aarch64-unknown-linux-gnu").as_deref(),
            Some("aarch64-unknown-linux-musl")
        );
        assert_eq!(musl_fallback_triple("x86_64-apple-darwin"), None);
    }

    #[test]
    fn linux_aliases_include_short_forms() {
        let aliases = target_aliases("x86_64-unknown-linux-gnu");
        assert!(aliases.contains(&"x86_64-linux".into()));
        assert!(aliases.contains(&"amd64-linux".into()));
        assert!(aliases.contains(&"linux-amd64".into()));
    }
}

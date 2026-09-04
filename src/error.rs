use std::fmt;
use std::io;
#[cfg(windows)]
use std::process::ExitStatus;

/// binox 可恢复错误。`code` 是进程退出码（exec 成功时不会走到这里）。
#[derive(Debug)]
pub struct Error {
    pub message: String,
    pub code: i32,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }

    pub fn with_code(code: i32, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::new(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::new(format!("网络请求失败: {err}"))
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::new(format!("JSON 解析失败: {err}"))
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(err: zip::result::ZipError) -> Self {
        Self::new(format!("zip 解压失败: {err}"))
    }
}

/// 把 `ExitStatus` 转成进程退出码（Windows spawn 路径用）。
#[cfg(windows)]
pub fn status_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

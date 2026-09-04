use crate::error::{Error, Result};
use crate::util::log;
use reqwest::blocking::Client;
use reqwest::header::LOCATION;
use reqwest::redirect::Policy;
use std::fs::File;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

const UA: &str = concat!(
    "binox/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/spraylee/binox)"
);

pub struct Http {
    follow: Client,
    no_redirect: Client,
}

impl Http {
    pub fn new() -> Result<Self> {
        Self::with_timeouts(Duration::from_secs(3), Duration::from_secs(120))
    }

    /// 自更新：探测短超时，下载仍给足时间，但不走主流程那套 3 次重试。
    pub fn for_self_update() -> Result<Self> {
        Self::with_timeouts(Duration::from_secs(3), Duration::from_secs(60))
    }

    fn with_timeouts(probe: Duration, download: Duration) -> Result<Self> {
        let follow = Client::builder()
            .user_agent(UA)
            .redirect(Policy::limited(10))
            .timeout(download)
            .build()?;
        let no_redirect = Client::builder()
            .user_agent(UA)
            .redirect(Policy::none())
            .timeout(probe)
            .build()?;
        Ok(Self {
            follow,
            no_redirect,
        })
    }

    /// 不跟随重定向，只读 Location（GitHub /releases/latest 的 302）。不重试。
    pub fn probe_location(&self, url: &str) -> Result<Option<String>> {
        let resp = self.no_redirect.get(url).send()?;
        let loc = resp
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        if loc.is_none() && resp.status().is_success() {
            return Ok(None);
        }
        if loc.is_none() && resp.status().is_redirection() {
            return Ok(None);
        }
        if !resp.status().is_redirection() && !resp.status().is_success() {
            return Err(Error::new(format!(
                "探测 {url} 失败: HTTP {}",
                resp.status()
            )));
        }
        Ok(loc)
    }

    pub fn get_text(&self, url: &str) -> Result<String> {
        let resp = self.follow.get(url).send()?;
        let status = resp.status();
        let text = resp.text()?;
        if !status.is_success() {
            return Err(Error::new(format!(
                "GET {url} 失败: HTTP {status} {}",
                truncate(&text, 200)
            )));
        }
        Ok(text)
    }

    pub fn download(&self, url: &str, dest: &Path) -> Result<()> {
        self.download_attempts(url, dest, 3)
    }

    /// 自更新下载：失败最多再试 1 次（离线冷启动不空转）。
    pub fn download_once_or_retry(&self, url: &str, dest: &Path) -> Result<()> {
        self.download_attempts(url, dest, 2)
    }

    fn download_attempts(&self, url: &str, dest: &Path, max: u32) -> Result<()> {
        let mut last_err = None;
        for attempt in 1..=max {
            match self.download_once(url, dest) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_err = Some(err);
                    if attempt < max {
                        log(format!("下载失败，重试 {attempt}/{max}: {url}"));
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| Error::new("下载失败")))
    }

    fn download_once(&self, url: &str, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = dest.with_extension("download-part");
        let mut resp = self.follow.get(url).send()?;
        let status = resp.status();
        if !status.is_success() {
            let _ = std::fs::remove_file(&part);
            return Err(Error::new(format!("下载失败 {url}: HTTP {status}")));
        }
        let mut file = File::create(&part)?;
        io::copy(&mut resp, &mut file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&part, dest)?;
        Ok(())
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max {
        s
    } else {
        format!("{}…", &s[..max])
    }
}

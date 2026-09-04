use crate::cli::Cli;
use crate::error::Result;
use crate::github;
use crate::http::Http;
use crate::platform;
use crate::update;
use crate::util::log;
use std::path::Path;
use std::process::Command;

pub fn run(cli: Cli) -> Result<()> {
    update::spawn_background_self_update();
    let http = Http::new()?;
    let platform = platform::detect()?;
    log(format!("平台 {}", platform.triple));
    let binary = github::prepare(
        &http,
        &cli.owner,
        &cli.repo,
        cli.version.as_deref(),
        cli.bin.as_deref(),
        cli.asset_template.as_deref(),
        cli.offline,
        &platform,
    )?;
    exec_binary(&binary, &cli.args)
}

pub fn exec_binary(binary: &Path, args: &[String]) -> Result<()> {
    log(format!("exec {} {}", binary.display(), args.join(" ")));
    let _ = std::io::Write::flush(&mut std::io::stderr());
    let _ = std::io::Write::flush(&mut std::io::stdout());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(binary).args(args).exec();
        Err(crate::error::Error::new(format!(
            "exec 失败 {}: {err}",
            binary.display()
        )))
    }

    #[cfg(windows)]
    {
        let status = Command::new(binary).args(args).status()?;
        std::process::exit(crate::error::status_code(status));
    }
}

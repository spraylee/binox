mod archive;
mod cache;
mod cli;
mod error;
mod github;
mod http;
mod locate;
mod match_asset;
mod platform;
mod runner;
mod update;
mod util;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("--self-update-worker") {
        if let Err(err) = update::run_worker() {
            eprintln!("[binox] {err}");
            std::process::exit(err.code);
        }
        return;
    }
    match cli::parse_args(args) {
        Ok(cli::Command::Help) => cli::print_help(),
        Ok(cli::Command::Version) => {
            println!("binox {}", env!("CARGO_PKG_VERSION"));
        }
        Ok(cli::Command::Update) => {
            if let Err(err) = update::run_foreground() {
                eprintln!("[binox] {err}");
                std::process::exit(err.code);
            }
        }
        Ok(cli::Command::Run(cli)) => {
            if let Err(err) = runner::run(cli) {
                eprintln!("[binox] {err}");
                std::process::exit(err.code);
            }
        }
        Err(err) => {
            eprintln!("[binox] {err}");
            std::process::exit(err.code);
        }
    }
}

//! Publishes synthetic dviz data (see `dorobot_plugin_dviz::fake`) until
//! Ctrl-C:
//!
//! cargo run --locked -p dorobot-plugin-dviz --example fake_publisher -- \
//!     --listen tcp/0.0.0.0:7447 [--connect tcp/host:7447] [--prefix dviz] [--hz 10]

use dorobot_plugin_dviz::fake::{run_for, FakeOptions};
use std::time::Duration;

const USAGE: &str =
    "usage: fake_publisher [--listen ENDPOINT]... [--connect ENDPOINT]... [--prefix NAME] [--hz N]";

fn parse(args: impl Iterator<Item = String>) -> Result<FakeOptions, String> {
    let mut options = FakeOptions::default();
    let mut args = args;
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value\n{USAGE}"));
        match flag.as_str() {
            "--listen" => options.listen_endpoints.push(value()?),
            "--connect" => options.connect_endpoints.push(value()?),
            "--prefix" => options.prefix = value()?,
            "--hz" => {
                options.hz = value()?
                    .parse()
                    .map_err(|_| format!("--hz needs a number\n{USAGE}"))?
            }
            _ => return Err(format!("unknown argument {flag}\n{USAGE}")),
        }
    }
    Ok(options)
}

fn main() {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    println!(
        "Publishing on {}/** ({} Hz). Ctrl-C to stop.",
        options.prefix, options.hz
    );
    // Ctrl-C ends the process; the duration is just "very long".
    if let Err(error) = run_for(options, Duration::from_secs(60 * 60 * 24 * 365)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

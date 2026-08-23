use anyhow::Result;
use std::ffi::OsString;

fn main() {
    if let Err(error) = run(std::env::args_os().collect()) {
        eprintln!("wts: {error:#}");
        std::process::exit(1);
    }
}

fn run(mut args: Vec<OsString>) -> Result<()> {
    if args
        .get(1)
        .is_some_and(|command| command == "serve" || command == "api")
    {
        args[0] = "wts".into();
        wt_server::run_from(args)
    } else {
        args[0] = "wts".into();
        wt_server_installer::run_from(args)
    }
}

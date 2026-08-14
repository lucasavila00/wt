fn main() {
    let target = match wt_guest::app_target() {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let status = match wt_guest::pane_command(&target).status() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("wt: could not start the devcontainer SSH command: {error}");
            eprintln!("wt: fix the error above, close this pane, and create a new one");
            std::process::exit(1);
        }
    };
    if let Some(diagnostic) = wt_guest::pane_failure_diagnostic(&status) {
        eprintln!("{diagnostic}");
        std::process::exit(status.code().unwrap_or(1));
    }
}

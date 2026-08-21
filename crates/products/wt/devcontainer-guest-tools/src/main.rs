fn main() {
    let target = match wt_devcontainer_guest_tools::app_target() {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let pane_id = std::env::var("TMUX_PANE").ok();
    let mut command = match wt_devcontainer_guest_tools::pane_command(&target, pane_id.as_deref()) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let status = match command.status() {
        Ok(status) => status,
        Err(error) => {
            eprintln!("wt: could not start the devcontainer SSH command: {error}");
            eprintln!("wt: fix the error above, close this pane, and create a new one");
            std::process::exit(1);
        }
    };
    if let Some(diagnostic) = wt_devcontainer_guest_tools::pane_failure_diagnostic(&status) {
        eprintln!("{diagnostic}");
        std::process::exit(status.code().unwrap_or(1));
    }
}

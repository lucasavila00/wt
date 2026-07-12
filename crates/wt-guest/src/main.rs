use std::os::unix::process::CommandExt;

fn main() {
    let target = match wt_guest::app_target() {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let error = wt_guest::pane_command(&target).exec();
    eprintln!("wt: start the devcontainer SSH shell: {error}");
    std::process::exit(1);
}

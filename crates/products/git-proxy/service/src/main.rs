fn main() {
    let _lock =
        match wt_git_proxy::ProcessLock::acquire(std::path::Path::new(wt_git_proxy::LOCK_PATH)) {
            Ok(lock) => lock,
            Err(error) => {
                eprintln!("wt-git-proxy: {error:#}");
                std::process::exit(1);
            }
        };
    if let Err(error) = wt_git_proxy::run(std::env::args_os()) {
        eprintln!("wt-git-proxy: {error:#}");
        std::process::exit(1);
    }
}

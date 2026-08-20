fn main() {
    if let Err(error) = wt_git_proxy::run(std::env::args_os()) {
        eprintln!("wt-git-proxy: {error:#}");
        std::process::exit(1);
    }
}

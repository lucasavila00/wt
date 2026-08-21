mod atomic_file;
pub mod config;
pub mod connection;
pub mod inventory;
pub mod ssh;
mod ssh_config;
pub mod transport;

#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $argument:expr)* $(,)?) => {{
        let mut command = ::std::process::Command::new($program);
        $(command.arg($argument);)*
        command
    }};
}

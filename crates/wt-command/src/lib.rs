/// Builds an owned [`std::process::Command`] from heterogeneous arguments.
#[macro_export]
macro_rules! cmd {
    ($program:expr $(, $argument:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut command = ::std::process::Command::new($program);
        $(command.arg($argument);)*
        command
    }};
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn builds_commands_from_heterogeneous_arguments() {
        let owner = "root";
        let mode = 0o640;
        let destination = Path::new("/tmp/example");
        let command = crate::cmd!(
            "install",
            "-o",
            owner,
            "-m",
            format!("{mode:04o}"),
            destination,
        );

        assert_eq!(command.get_program(), OsStr::new("install"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-o", "root", "-m", "0640", "/tmp/example"]
        );
    }

    #[test]
    fn builds_commands_without_arguments() {
        let command = crate::cmd!("true",);
        assert_eq!(command.get_program(), OsStr::new("true"));
        assert_eq!(command.get_args().count(), 0);
    }
}

mod codegen;

use std::{env, fs, path::PathBuf};

fn main() {
    const CONTRACT: &str = "src/api/wt-tools-command.ts";
    println!("cargo:rerun-if-changed={CONTRACT}");
    let source = fs::read_to_string(CONTRACT).expect("read TypeScript command contract");
    let rust = codegen::generate(CONTRACT, source).unwrap_or_else(|error| panic!("{error}"));
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set")).join("wt_tools_command.rs"),
        rust,
    )
    .expect("write generated Rust command types");
}

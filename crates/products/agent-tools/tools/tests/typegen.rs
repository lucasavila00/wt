#[path = "../codegen.rs"]
mod codegen;

const CONTRACT: &str = include_str!("../src/api/wt-tools-command.ts");

#[test]
fn generated_rust_is_the_complete_command_contract() {
    insta::assert_snapshot!(codegen::generate("wt-tools-command.ts", CONTRACT.to_owned()).unwrap());
}

#[test]
fn rejects_types_outside_the_command_contract() {
    for (name, source) in [
        ("interface", "export interface Command {}"),
        ("generic", "export type Value<T> = T;"),
        (
            "missing_action",
            "export type WtToolsCommand = { value: string } | { action: \"ok\" };",
        ),
        (
            "optional_action",
            "export type WtToolsCommand = { action?: \"show_mr\"; mr: string } | { action: \"ok\" };",
        ),
        (
            "duplicate_action",
            "export type WtToolsCommand = { action: \"show_mr\" } | { action: \"show_mr\"; mr: string };",
        ),
        (
            "rust_keyword",
            "export type WtToolsCommand = { action: \"show_mr\"; type: string } | { action: \"ok\" };",
        ),
        (
            "unsupported_number",
            "export type WtToolsCommand = { action: \"show_mr\"; value: number } | { action: \"ok\" };",
        ),
    ] {
        insta::assert_snapshot!(name, codegen::generate("contract.ts", source.to_owned()).unwrap_err());
    }
}

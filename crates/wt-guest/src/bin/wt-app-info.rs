fn main() {
    match wt_guest::app_target().and_then(|target| {
        serde_json::to_string(&target).map_err(|error| format!("wt: encode app target: {error}"))
    }) {
        Ok(target) => println!("{target}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

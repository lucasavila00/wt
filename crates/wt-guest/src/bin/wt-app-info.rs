fn main() {
    let field = std::env::args().nth(1);
    match wt_guest::app_target().and_then(|target| match field.as_deref() {
        None => serde_json::to_string(&target)
            .map_err(|error| format!("wt: encode app target: {error}")),
        Some("user") => Ok(target.user),
        Some("address") => Ok(target.address),
        Some(_) => Err("wt: usage: wt-app-info [user|address]".to_owned()),
    }) {
        Ok(target) => println!("{target}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

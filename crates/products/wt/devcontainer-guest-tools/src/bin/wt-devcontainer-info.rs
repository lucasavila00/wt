use std::io::Read;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match args.as_slice() {
        [command] if command == "configured-user" => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .map_err(|error| format!("wt: read devcontainer configuration: {error}"))
                .and_then(|_| wt_devcontainer_guest_tools::configured_remote_user(&input))
                .map(Some)
        }
        [command, expected] if command == "verify-user" => {
            wt_devcontainer_guest_tools::verify_app_user(expected).map(|_| None)
        }
        [] | [_] => wt_devcontainer_guest_tools::app_target()
            .and_then(|target| match args.first().map(String::as_str) {
                None => serde_json::to_string(&target)
                    .map_err(|error| format!("wt: encode app target: {error}")),
                Some("user") => Ok(target.user),
                Some("address") => Ok(target.address),
                Some(_) => Err(usage()),
            })
            .map(Some),
        _ => Err(usage()),
    };
    match result {
        Ok(Some(target)) => println!("{target}"),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn usage() -> String {
    "wt: usage: wt-app-info [user|address|configured-user|verify-user USER]".to_owned()
}

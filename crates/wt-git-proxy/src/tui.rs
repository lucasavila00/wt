use crate::{add_generated_client, list_keys, remove_key, ProxyConfig};
use anyhow::{bail, Context, Result};
use console::{Key, Term};
use std::io::IsTerminal;
use std::path::Path;

#[derive(Debug, Eq, PartialEq)]
enum DashboardAction {
    Generate,
    Revoke,
    Quit,
}

pub fn run_tui(config_path: &Path) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("wt-git-proxy tui requires an interactive terminal");
    }
    let config = ProxyConfig::load(config_path)?;
    let executable = std::env::current_exe().context("find proxy executable")?;
    let term = Term::stdout();

    loop {
        show_dashboard(&term, config_path, &config)?;
        match dashboard_action(term.read_key().context("read dashboard key")?) {
            Some(DashboardAction::Generate) => {
                let client = add_generated_client(config_path, &executable, &config)?;
                term.write_line("")?;
                term.write_line("Client authorized. Paste this secret command into the agent VM:")?;
                term.write_line("")?;
                term.write_line(&client.install_command)?;
                term.write_line("")?;
                term.write_line(&format!(
                    "Client: {} ({})",
                    client.alias, client.fingerprint
                ))?;
                term.write_line(
                    "The command contains a private key. Do not save it in logs or shell history.",
                )?;
            }
            Some(DashboardAction::Revoke) => revoke_client(config_path, &executable)?,
            Some(DashboardAction::Quit) => break,
            None => {}
        }
    }
    Ok(())
}

fn show_dashboard(term: &Term, config_path: &Path, config: &ProxyConfig) -> Result<()> {
    let count = list_keys(config_path)?.len();
    term.write_line("")?;
    term.write_str(&dashboard(count, &config.client_host, config.client_port))?;
    Ok(())
}

fn dashboard(count: usize, proxy_host: &str, proxy_port: u16) -> String {
    format!(
        "WT Git proxy\n\
Proxy address: git-proxy@{proxy_host}:{proxy_port} (confirmed during setup)\n\
The agent must be able to resolve and reach this address.\n\
\n\
Press Space to give one agent access. You will get one command to paste into\n\
that agent. The command saves its new SSH key under ~/.ssh/wt-git-proxy and\n\
updates Git and SSH for that user. Existing GitHub checkouts will use this\n\
proxy without changing their saved remotes. The GitHub key stays on this server.\n\
\n\
Press R to stop an agent from connecting again.\n\
\n\
Authorized clients: {count}\n\
\n\
SPACE  Generate and authorize an agent key\n\
R      Revoke an agent key\n\
Q      Quit\n"
    )
}

fn dashboard_action(key: Key) -> Option<DashboardAction> {
    match key {
        Key::Char(' ') => Some(DashboardAction::Generate),
        Key::Char('r' | 'R') => Some(DashboardAction::Revoke),
        Key::Char('q' | 'Q') | Key::Escape | Key::CtrlC => Some(DashboardAction::Quit),
        _ => None,
    }
}

fn revoke_client(config_path: &Path, executable: &Path) -> Result<()> {
    let keys = list_keys(config_path)?;
    if keys.is_empty() {
        cliclack::note("Nothing to revoke", "No clients are authorized")?;
        return Ok(());
    }
    let mut selected = cliclack::select("Client to revoke");
    for key in keys {
        selected = selected.item(
            key.fingerprint.clone(),
            format!("{} ({})", key.label, key.fingerprint),
            "",
        );
    }
    let fingerprint = selected.interact()?;
    remove_key(config_path, executable, &fingerprint)?;
    cliclack::note("Client revoked", fingerprint)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_is_not_a_form() {
        insta::assert_snapshot!(dashboard(2, "proxy.example.com", 2222), @"
        WT Git proxy
        Proxy address: git-proxy@proxy.example.com:2222 (confirmed during setup)
        The agent must be able to resolve and reach this address.

        Press Space to give one agent access. You will get one command to paste into
        that agent. The command saves its new SSH key under ~/.ssh/wt-git-proxy and
        updates Git and SSH for that user. Existing GitHub checkouts will use this
        proxy without changing their saved remotes. The GitHub key stays on this server.

        Press R to stop an agent from connecting again.

        Authorized clients: 2

        SPACE  Generate and authorize an agent key
        R      Revoke an agent key
        Q      Quit
        ");
    }

    #[test]
    fn space_generates_without_enter() {
        assert_eq!(
            dashboard_action(Key::Char(' ')),
            Some(DashboardAction::Generate)
        );
        assert_eq!(
            dashboard_action(Key::Char('R')),
            Some(DashboardAction::Revoke)
        );
        assert_eq!(
            dashboard_action(Key::Char('q')),
            Some(DashboardAction::Quit)
        );
    }
}

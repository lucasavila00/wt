use super::*;
use crate::server::binaries;

pub(in crate::image) fn verify_guest_contract(runner: &impl Runner, disk: &Path) -> Result<()> {
    let contract = runner.text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            disk,
            "/usr/local/share/wt-guest-contract"
        ),
        "inspect finalized guest contract",
    )?;
    let expected_contract = format!(
        "WT_USER='{}'\nWT_GROUP='{}'\nWT_UID='{}'\nWT_GID='{}'\nWT_HOME='{}'\n",
        wt_guest::GUEST_USER,
        wt_guest::GUEST_GROUP,
        wt_guest::GUEST_UID,
        wt_guest::GUEST_GID,
        wt_guest::GUEST_HOME,
    );
    if contract != expected_contract {
        bail!("finalized image guest contract differs from policy");
    }

    let binary_listing = runner.text(
        cmd!(
            "sudo",
            "virt-ls",
            "--long",
            "--recursive",
            "--uids",
            "-a",
            disk,
            "/usr/local/bin"
        ),
        "inspect finalized guest binary metadata",
    )?;
    for (name, _) in super::super::GUEST_BINARY_INPUTS {
        let guest_path = format!("/usr/local/bin/{name}");
        let fields = metadata_fields(&binary_listing, &guest_path);
        if fields.len() < 6
            || fields[0] != "-"
            || fields[1] != "0755"
            || fields[3] != "0"
            || fields[4] != "0"
        {
            bail!("finalized image guest binary must be root:root 0755: {guest_path}");
        }
        let output = runner.output(cmd!("sudo", "virt-cat", "-a", disk, &guest_path))?;
        if !output.status.success() {
            bail!("finalized image does not contain {guest_path}");
        }
        let expected = fs::read(binaries::guest_binary())
            .with_context(|| format!("read built guest binary {name}"))?;
        if output.stdout != expected {
            bail!("finalized image guest binary differs: {guest_path}");
        }
    }
    for (path, target) in [
        ("/usr/local/bin/git-remote-wt-agent", "/usr/local/bin/wtg"),
        ("/usr/local/bin/codex", "/home/wt/.local/bin/codex"),
    ] {
        let expected = format!(" {path} -> {target}");
        if !binary_listing
            .lines()
            .any(|line| line.starts_with("l 0777 ") && line.ends_with(&expected))
        {
            bail!("finalized image guest alias differs: {path}");
        }
    }

    let passwd = runner.text(
        cmd!("sudo", "virt-cat", "-a", disk, "/etc/passwd"),
        "inspect finalized guest users",
    )?;
    let expected_user = format!(
        "{}:x:{}:{}:",
        wt_guest::GUEST_USER,
        wt_guest::GUEST_UID,
        wt_guest::GUEST_GID
    );
    let expected_home = format!(":{}:", wt_guest::GUEST_HOME);
    if !passwd
        .lines()
        .any(|line| line.starts_with(&expected_user) && line.contains(&expected_home))
    {
        bail!("finalized image does not contain the required guest user");
    }
    let group = runner.text(
        cmd!("sudo", "virt-cat", "-a", disk, "/etc/group"),
        "inspect finalized guest groups",
    )?;
    let expected_group = format!("{}:x:{}:", wt_guest::GUEST_GROUP, wt_guest::GUEST_GID);
    if !group.lines().any(|line| line == expected_group) {
        bail!("finalized image does not contain the required guest group");
    }

    let color = runner.output(cmd!(
        "sudo",
        "virt-cat",
        "-a",
        disk,
        format!("{}/.byobu/color", wt_guest::GUEST_HOME)
    ))?;
    if !color.status.success() || color.stdout != BYOBU_COLOR {
        bail!("finalized image guest Byobu color differs from policy");
    }
    let listing = runner.text(
        cmd!(
            "sudo",
            "virt-ls",
            "--long",
            "--recursive",
            "--uids",
            "-a",
            disk,
            wt_guest::GUEST_HOME
        ),
        "inspect finalized guest home",
    )?;
    let color_path = format!("{}/.byobu/color", wt_guest::GUEST_HOME);
    let fields = listing
        .lines()
        .find(|line| line.ends_with(&format!(" {color_path}")))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    if fields.len() < 6
        || fields[0] != "-"
        || fields[1] != "0644"
        || fields[3] != wt_guest::GUEST_UID.to_string()
        || fields[4] != wt_guest::GUEST_GID.to_string()
    {
        bail!("finalized guest Byobu color must be owned by the guest user with mode 0644");
    }
    Ok(())
}

fn metadata_fields<'a>(listing: &'a str, path: &str) -> Vec<&'a str> {
    listing
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .find(|fields| fields.get(5).is_some_and(|candidate| *candidate == path))
        .unwrap_or_default()
}

pub(in crate::image) fn validate_result_metadata(listing: &str) -> Result<()> {
    let fields = listing
        .lines()
        .find(|line| line.ends_with(" /var/lib/wt-image-result"))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    if fields.len() < 6
        || fields[0] != "-"
        || fields[1] != "0644"
        || fields[3] != "0"
        || fields[4] != "0"
    {
        bail!("image build result must be owned by root:root with mode 0644");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::metadata_fields;

    #[test]
    fn binary_metadata_ignores_symlinks_that_target_the_binary() {
        let guest_path = "/usr/local/bin/wtg";
        let listing = concat!(
            "l 0777 18 0 0 /usr/local/bin/git-remote-wt-agent -> /usr/local/bin/wtg\n",
            "- 0755 123 0 0 /usr/local/bin/wtg\n",
        );
        let fields = metadata_fields(listing, guest_path);

        assert_eq!(fields[0], "-");
        assert_eq!(fields[1], "0755");
    }
}

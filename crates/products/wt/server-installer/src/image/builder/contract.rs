use super::*;

pub(in crate::image) fn verify_retained_guest_contract(
    runner: &impl Runner,
    disk: &Path,
) -> Result<()> {
    let contract = runner.text(
        cmd!(
            "sudo",
            "virt-cat",
            "-a",
            disk,
            "/usr/local/share/wt-retained-contract"
        ),
        "inspect finalized retained guest contract",
    )?;
    let expected_contract = format!(
        "WT_USER='{}'\nWT_UID='{}'\nWT_GID='{}'\nWT_HOME='{}'\n",
        wt_retained_worlds::GUEST_USER,
        wt_retained_worlds::GUEST_UID,
        wt_retained_worlds::GUEST_GID,
        wt_retained_worlds::GUEST_HOME,
    );
    if contract != expected_contract {
        bail!("finalized image retained guest contract differs from policy");
    }

    let passwd = runner.text(
        cmd!("sudo", "virt-cat", "-a", disk, "/etc/passwd"),
        "inspect finalized guest users",
    )?;
    let expected_user = format!(
        "{}:x:{}:{}:",
        wt_retained_worlds::GUEST_USER,
        wt_retained_worlds::GUEST_UID,
        wt_retained_worlds::GUEST_GID
    );
    let expected_home = format!(":{}:", wt_retained_worlds::GUEST_HOME);
    if !passwd
        .lines()
        .any(|line| line.starts_with(&expected_user) && line.contains(&expected_home))
    {
        bail!("finalized image does not contain the required retained guest user");
    }

    let color = runner.output(cmd!(
        "sudo",
        "virt-cat",
        "-a",
        disk,
        format!("{}/.byobu/color", wt_retained_worlds::GUEST_HOME)
    ))?;
    if !color.status.success() || color.stdout != BYOBU_COLOR {
        bail!("finalized image retained guest Byobu color differs from policy");
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
            wt_retained_worlds::GUEST_HOME
        ),
        "inspect finalized retained guest home",
    )?;
    let color_path = format!("{}/.byobu/color", wt_retained_worlds::GUEST_HOME);
    let fields = listing
        .lines()
        .find(|line| line.ends_with(&format!(" {color_path}")))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    if fields.len() < 6
        || fields[0] != "-"
        || fields[1] != "0644"
        || fields[3] != wt_retained_worlds::GUEST_UID.to_string()
        || fields[4] != wt_retained_worlds::GUEST_GID.to_string()
    {
        bail!(
            "finalized retained guest Byobu color must be owned by the guest user with mode 0644"
        );
    }
    Ok(())
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

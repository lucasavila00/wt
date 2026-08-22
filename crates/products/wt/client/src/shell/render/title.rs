pub(super) fn refresh_title(
    label: &str,
    updated_at: Option<&str>,
    failure: Option<&[String]>,
) -> String {
    let mut title = updated_at.map_or_else(
        || format!("{label} · Updating…"),
        |updated_at| format!("{label} · Last updated {updated_at}"),
    );
    if let Some(failures) = failure {
        title.push_str(" · Sync failed: ");
        title.push_str(&failures.join("; "));
    }
    title
}

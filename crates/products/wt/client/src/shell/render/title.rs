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

pub(super) fn worlds_refresh_title(updated_at: Option<&str>, failure: Option<&str>) -> String {
    if let Some(error) = failure {
        return updated_at.map_or_else(
            || format!("Worlds · Refresh failed: {error}"),
            |updated_at| {
                format!("Worlds · Refresh failed: {error} · Showing data from {updated_at}")
            },
        );
    }
    updated_at.map_or_else(
        || "Worlds · Updating…".into(),
        |updated_at| format!("Worlds · Last updated {updated_at}"),
    )
}

#[derive(Debug, Default)]
pub(super) struct RefreshStatus {
    updated_at: Option<String>,
    failures: Option<Vec<String>>,
}

impl RefreshStatus {
    pub(super) fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }

    pub(super) fn failures(&self) -> Option<&[String]> {
        self.failures.as_deref()
    }

    pub(super) fn finish(&mut self, result: Result<String, Vec<String>>) {
        match result {
            Ok(updated_at) => {
                self.updated_at = Some(updated_at);
                self.failures = None;
            }
            Err(failures) => self.failures = Some(failures),
        }
    }

    pub(super) fn set_failures(&mut self, failures: Vec<String>) {
        self.failures = (!failures.is_empty()).then_some(failures);
    }

    pub(super) fn title(&self, label: &str) -> String {
        self.updated_at().map_or_else(
            || format!("{label} · Updating…"),
            |updated_at| format!("{label} · Last updated {updated_at}"),
        )
    }

    pub(super) fn failure(&self) -> Option<String> {
        self.failures()
            .map(|failures| format!(" · Sync failed: {}", failures.join("; ")))
    }
}

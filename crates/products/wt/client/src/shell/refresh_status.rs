use std::time::{Duration, Instant};

pub(super) const PANE_REFRESH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(super) struct RefreshStatus {
    updated_at: Option<String>,
    failures: Option<Vec<String>>,
    last_successful_update: Instant,
    stale: bool,
}

impl Default for RefreshStatus {
    fn default() -> Self {
        Self {
            updated_at: None,
            failures: None,
            last_successful_update: Instant::now(),
            stale: false,
        }
    }
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
                self.last_successful_update = Instant::now();
                self.stale = false;
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

    pub(super) fn update_staleness(&mut self, timeout: Duration) -> bool {
        let stale = self.last_successful_update.elapsed() >= timeout;
        let changed = stale != self.stale;
        self.stale = stale;
        changed
    }

    pub(super) fn is_stale(&self) -> bool {
        self.stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_a_refresh_stale_after_its_timeout() {
        let mut status = RefreshStatus {
            last_successful_update: Instant::now() - PANE_REFRESH_TIMEOUT,
            ..Default::default()
        };

        assert!(status.update_staleness(PANE_REFRESH_TIMEOUT));
        assert!(status.is_stale());
    }

    #[test]
    fn a_successful_update_restores_a_stale_refresh() {
        let mut status = RefreshStatus {
            last_successful_update: Instant::now() - PANE_REFRESH_TIMEOUT,
            ..Default::default()
        };
        status.update_staleness(PANE_REFRESH_TIMEOUT);

        status.finish(Ok("2026-08-28T00:00:00Z".into()));

        assert!(!status.is_stale());
    }
}

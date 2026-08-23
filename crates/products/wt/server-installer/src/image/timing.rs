use anyhow::Result;
use std::process::Command;
use std::time::Instant;
use wt_installer_support::Runner;

pub(super) fn timed<T>(action: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    println!("Image phase: {action}...");
    let started = Instant::now();
    let result = operation();
    let elapsed = started.elapsed().as_secs_f64();
    match &result {
        Ok(_) => println!("Image phase complete: {action} ({elapsed:.1}s)."),
        Err(_) => eprintln!("Image phase failed: {action} ({elapsed:.1}s)."),
    }
    result
}

pub(super) trait TimedRunner: Runner {
    fn timed_run(&self, command: Command, action: &str) -> Result<()> {
        timed(action, || self.run(command, action))
    }

    fn timed_text(&self, command: Command, action: &str) -> Result<String> {
        timed(action, || self.text(command, action))
    }
}

impl<R: Runner + ?Sized> TimedRunner for R {}

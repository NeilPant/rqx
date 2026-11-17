//! Work that runs off the input thread, so typing and the owl stay live while it does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use anyhow::Result;

use crate::scan::{ScanProgress, Watch};

pub struct Job<T> {
    worker: JoinHandle<Result<T>>,
    updates: Receiver<ScanProgress>,
    cancel: Arc<AtomicBool>,
}

pub enum Finished<T> {
    Done(T),
    /// Asked to stop. Nothing went wrong, and nothing the job had is worth keeping.
    Cancelled,
    Failed(String),
}

impl<T: Send + 'static> Job<T> {
    /// `work` is handed the scan's watch, so a job that scans reports progress and
    /// honours a cancel, and one that does not simply ignores it.
    pub fn start(work: impl FnOnce(Watch) -> Result<T> + Send + 'static) -> Self {
        let (sender, updates) = mpsc::sync_channel(1);
        let watch = Watch::new(sender);
        let cancel = Arc::clone(&watch.cancel);
        Self {
            worker: thread::spawn(move || work(watch)),
            updates,
            cancel,
        }
    }

    /// The newest progress since the last call. The scan sends one per file, so only
    /// the last of them says anything a frame can show.
    pub fn progress(&self) -> Option<ScanProgress> {
        self.updates.try_iter().last()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn finished(&self) -> bool {
        self.worker.is_finished()
    }

    pub fn join(self) -> Finished<T> {
        let stopped = self.cancel.load(Ordering::Relaxed);
        match self.worker.join() {
            _ if stopped => Finished::Cancelled,
            Ok(Ok(value)) => Finished::Done(value),
            Ok(Err(error)) => Finished::Failed(format!("{error:#}")),
            Err(_) => Finished::Failed("the job stopped unexpectedly".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait<T: Send + 'static>(job: Job<T>) -> Finished<T> {
        while !job.finished() {
            thread::yield_now();
        }
        job.join()
    }

    #[test]
    fn a_finished_job_hands_back_its_value() {
        let job = Job::start(|_| Ok(7));
        assert!(matches!(wait(job), Finished::Done(7)));
    }

    #[test]
    fn a_failure_is_reported_as_text() {
        let job: Job<()> = Job::start(|_| anyhow::bail!("no such directory"));
        assert!(matches!(wait(job), Finished::Failed(message) if message == "no such directory"));
    }

    #[test]
    fn a_cancelled_job_is_neither_a_value_nor_a_failure() {
        let job: Job<()> = Job::start(|watch| {
            while !watch.cancel.load(Ordering::Relaxed) {
                thread::yield_now();
            }
            anyhow::bail!("the scan was cancelled")
        });
        job.cancel();
        assert!(matches!(wait(job), Finished::Cancelled));
    }
}

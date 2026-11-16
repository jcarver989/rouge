use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::Builder;
use std::time::Duration;

type Job = Box<dyn FnOnce() + Send>;

const IDLE: Duration = Duration::from_secs(10);

pub(crate) struct Pool {
    jobs: Sender<Job>,
    idle: Receiver<Job>,
}

impl Default for Pool {
    fn default() -> Self {
        let (jobs, idle) = bounded(0);
        Pool { jobs, idle }
    }
}

impl Pool {
    pub(crate) fn submit(&self, job: Job) {
        let Err(TrySendError::Full(job)) = self.jobs.try_send(job) else {
            return;
        };
        let idle = self.idle.clone();
        let worker = move || {
            let mut next = Some(job);
            while let Some(job) = next {
                let _ = catch_unwind(AssertUnwindSafe(job));
                next = idle.recv_timeout(IDLE).ok();
            }
        };
        let _ = Builder::new().name("rouge-blocking".into()).spawn(worker);
    }
}

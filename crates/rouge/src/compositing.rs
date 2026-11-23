use crate::app::AppError;
use crate::driver::Inline;
use rouge_core::{Events, Message, ThreadError, mailbox, spawn};
use rouge_render::{Gpu, Target};
use std::env::var;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Instant;

enum Host {
    Inline(Box<Inline>),
    Thread(JoinHandle<Vec<Target>>),
}

pub(crate) struct Compositing {
    pub(crate) messages: Sender<Message>,
    pub(crate) events: Events,
    pub(crate) epoch: Instant,
    host: Host,
}

impl Compositing {
    pub(crate) fn open(
        gpu: &Gpu,
        wake: impl Fn() + Clone + Send + Sync + 'static,
    ) -> Result<Self, AppError> {
        let (messages, mailbox, events) = mailbox();
        let epoch = Instant::now();
        let host = if var("ROUGE_COMPOSITOR").is_ok_and(|mode| mode == "inline") {
            Host::Inline(Box::new(Inline::new(gpu, mailbox, epoch, wake)))
        } else {
            Host::Thread(spawn(gpu.clone(), messages.clone(), mailbox, epoch, wake)?)
        };
        Ok(Compositing {
            messages,
            events,
            epoch,
            host,
        })
    }

    #[cfg(not(target_os = "android"))]
    pub(crate) fn threaded(&self) -> bool {
        matches!(self.host, Host::Thread(_))
    }

    pub(crate) fn turn(&mut self) -> Option<Instant> {
        match &mut self.host {
            Host::Inline(inline) => inline.turn(),
            Host::Thread(_) => None,
        }
    }

    pub(crate) fn close(self) -> Result<(), AppError> {
        if let Host::Thread(thread) = self.host {
            let _ = self.messages.send(Message::Shutdown);
            drop(thread.join().map_err(|_| ThreadError::Panicked)?);
        }
        Ok(())
    }
}

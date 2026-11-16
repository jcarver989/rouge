use super::actor::{CompositorActor, Message};
use super::link::Mailbox;
use rouge_render::{Gpu, Surfaces, Target};
use std::io;
use std::ops::ControlFlow;
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThreadError {
    #[error("failed to start the compositor thread: {0}")]
    Spawn(#[from] io::Error),
    #[error("the compositor thread panicked")]
    Panicked,
}

pub fn spawn(
    gpu: Gpu,
    messages: Sender<Message>,
    mailbox: Mailbox,
    epoch: Instant,
    wake: impl Fn() + Send + 'static,
) -> Result<JoinHandle<Vec<Target>>, ThreadError> {
    let run = move || {
        let surfaces = Surfaces::new(&gpu).with_waker(move || {
            let _ = messages.send(Message::Wake);
        });
        let mut actor = CompositorActor::new(gpu, surfaces);
        loop {
            let now = epoch.elapsed().as_secs_f64();
            let first = match actor.next_wake(now) {
                Some(due) => {
                    let wait = Duration::from_secs_f64((due - now).max(0.0));
                    mailbox.messages.recv_timeout(wait)
                }
                None => (mailbox.messages.recv()).map_err(|_| RecvTimeoutError::Disconnected),
            };
            match first {
                Ok(Message::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(message) => actor.handle(message),
                Err(RecvTimeoutError::Timeout) => {}
            }
            match mailbox.turn(&mut actor, epoch.elapsed().as_secs_f64()) {
                ControlFlow::Break(()) => break,
                ControlFlow::Continue(0) => {}
                ControlFlow::Continue(_) => wake(),
            }
        }
        actor.into_targets()
    };
    Ok(Builder::new().name("rouge-compositor".into()).spawn(run)?)
}

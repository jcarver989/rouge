use super::actor::{CompositorActor, Message, WindowId};
use super::commit::CompositorEvent;
use std::ops::ControlFlow;
use std::sync::mpsc::{Receiver, Sender, channel};

pub type Events = Receiver<(WindowId, CompositorEvent)>;

pub struct Mailbox {
    pub(super) messages: Receiver<Message>,
    events: Sender<(WindowId, CompositorEvent)>,
}

pub fn mailbox() -> (Sender<Message>, Mailbox, Events) {
    let (sender, messages) = channel();
    let (events, received) = channel();
    (sender, Mailbox { messages, events }, received)
}

impl Mailbox {
    pub fn turn(&self, actor: &mut CompositorActor, now: f64) -> ControlFlow<(), usize> {
        for message in self.messages.try_iter() {
            if let Message::Shutdown = message {
                return ControlFlow::Break(());
            }
            actor.handle(message);
        }
        actor.frame(now);
        let sent = actor
            .events()
            .filter(|event| self.events.send(event.clone()).is_ok());
        ControlFlow::Continue(sent.count())
    }
}

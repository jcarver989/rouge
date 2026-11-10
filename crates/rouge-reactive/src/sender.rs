use crate::handle::Handle;
use crate::task::Mailbox;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct SignalSender<T> {
    handle: Handle,
    mailbox: Arc<Mailbox>,
    marker: PhantomData<fn(T)>,
}

impl<T> Clone for SignalSender<T> {
    fn clone(&self) -> Self {
        SignalSender {
            handle: self.handle,
            mailbox: self.mailbox.clone(),
            marker: PhantomData,
        }
    }
}

impl<T> SignalSender<T> {
    pub(crate) fn new(handle: Handle, mailbox: Arc<Mailbox>) -> Self {
        SignalSender {
            handle,
            mailbox,
            marker: PhantomData,
        }
    }
}

impl<T: Send + 'static> SignalSender<T> {
    pub fn set(&self, value: T) {
        self.update(move |v| *v = value);
    }

    pub fn update(&self, f: impl FnOnce(&mut T) + Send + 'static) {
        let handle = self.handle;
        self.mailbox.post(Box::new(move || {
            handle.try_update(f);
        }));
    }
}

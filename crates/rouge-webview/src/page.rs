use crate::content::WebContent;
use crate::engine::{
    EnginePage, PageCommand, Placement, WebEngine, WebViewEvent, WebViewEvents, WebViewOptions,
};
use crate::error::WebViewError;
use rouge_reactive::{
    Callback, ReadSignal, Signal, Stored, effect, on_cleanup, signal, stored, use_context, watch,
};
use std::mem::take;
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub progress: f32,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub failure: Option<String>,
}

impl PageState {
    fn record(&mut self, event: WebViewEvent) {
        match event {
            WebViewEvent::Loading(loading) => {
                self.loading = loading;
                if loading {
                    self.failure = None;
                }
            }
            WebViewEvent::Progress(progress) => self.progress = progress,
            WebViewEvent::Url(url) => self.url = url,
            WebViewEvent::Title(title) => self.title = title,
            WebViewEvent::History { back, forward } => {
                (self.can_go_back, self.can_go_forward) = (back, forward);
            }
            WebViewEvent::Failed(reason) => {
                self.loading = false;
                self.failure = Some(reason);
            }
            WebViewEvent::Message(_) => {}
        }
    }
}

#[derive(Clone, Copy)]
pub struct WebPage {
    inner: Stored<Inner>,
    state: Signal<PageState>,
}

#[track_caller]
pub fn web_page(content: WebContent) -> WebPage {
    WebViewOptions::default().open(content)
}

impl WebViewOptions {
    #[track_caller]
    pub fn open(self, content: WebContent) -> WebPage {
        let inbox = signal(Vec::new());
        let sender = inbox.sender();
        let events = WebViewEvents::new(move |event| sender.update(move |queue| queue.push(event)));
        let opened = use_context::<Rc<dyn WebEngine>>()
            .ok_or(WebViewError::NoWindow)
            .and_then(|engine| engine.open(self, events));
        let (mut native, error) = match opened {
            Ok(native) => (native, None),
            Err(error) => (Box::new(Closed) as Box<dyn EnginePage>, Some(error)),
        };
        native.apply(PageCommand::Load(content.clone()));
        let page = WebPage {
            inner: stored(Inner {
                native,
                error,
                content,
                on_message: Callback::default(),
                mounts: Vec::new(),
                mounted: 0,
            }),
            state: signal(PageState::default()),
        };
        effect(move || {
            if !inbox.with(Vec::is_empty)
                && let Some(events) = inbox.try_update(take)
            {
                for event in events {
                    page.hear(event);
                }
            }
        });
        page
    }
}

impl WebPage {
    pub fn state(self) -> ReadSignal<PageState> {
        self.state.read_only()
    }

    pub fn error(self) -> Option<WebViewError> {
        self.with_inner(|inner| inner.error.clone()).flatten()
    }

    pub fn on_message(self, on_message: impl Into<Callback<(String,)>>) -> Self {
        let on_message = on_message.into();
        self.with_inner(|inner| inner.on_message = on_message);
        self
    }

    pub fn load(self, content: WebContent) {
        self.with_inner(|inner| {
            if inner.content != content {
                inner.native.apply(PageCommand::Load(content.clone()));
                inner.content = content;
            }
        });
    }

    pub fn reload(self) {
        self.send(PageCommand::Reload);
    }

    pub fn stop(self) {
        self.send(PageCommand::Stop);
    }

    pub fn go_back(self) {
        self.send(PageCommand::Back);
    }

    pub fn go_forward(self) {
        self.send(PageCommand::Forward);
    }

    pub fn post_message(self, message: impl Into<String>) {
        self.send(PageCommand::Post(message.into()));
    }

    pub(crate) fn attach(self, placement: impl Fn() -> Placement + 'static) {
        let Some(mount) = self.with_inner(Inner::mount) else {
            return;
        };
        on_cleanup(move || {
            self.with_inner(|inner| inner.unmount(mount));
        });
        watch(placement, move |placed, _| {
            self.with_inner(|inner| inner.place(mount, *placed));
        });
    }

    fn send(self, command: PageCommand) {
        self.with_inner(|inner| inner.native.apply(command));
    }

    fn hear(self, event: WebViewEvent) {
        match event {
            WebViewEvent::Message(message) => {
                if let Some(on_message) = self.with_inner(|inner| inner.on_message) {
                    on_message.call(message);
                }
            }
            event => self.state.update(|state| state.record(event)),
        }
    }

    fn with_inner<T>(self, use_inner: impl FnOnce(&mut Inner) -> T) -> Option<T> {
        self.inner.is_alive().then(|| self.inner.update(use_inner))
    }
}

struct Inner {
    native: Box<dyn EnginePage>,
    error: Option<WebViewError>,
    content: WebContent,
    on_message: Callback<(String,)>,
    mounts: Vec<Mount>,
    mounted: u64,
}

struct Mount {
    id: u64,
    placement: Placement,
}

impl Inner {
    fn mount(&mut self) -> u64 {
        self.mounted += 1;
        self.mounts.push(Mount {
            id: self.mounted,
            placement: Placement::default(),
        });
        self.mounted
    }

    fn place(&mut self, id: u64, placement: Placement) {
        let Some(index) = self.mounts.iter().position(|mount| mount.id == id) else {
            return;
        };
        self.mounts[index].placement = placement;
        if index + 1 == self.mounts.len() {
            self.native.apply(PageCommand::Place(placement));
        }
    }

    fn unmount(&mut self, id: u64) {
        let Some(index) = self.mounts.iter().position(|mount| mount.id == id) else {
            return;
        };
        self.mounts.remove(index);
        if index == self.mounts.len() {
            let shown = self.mounts.last().map_or_default(|mount| mount.placement);
            self.native.apply(PageCommand::Place(shown));
        }
    }
}

struct Closed;

impl EnginePage for Closed {
    fn apply(&mut self, _command: PageCommand) {}
}

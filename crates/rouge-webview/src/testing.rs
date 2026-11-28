use crate::content::WebContent;
use crate::engine::{
    EnginePage, PageCommand, Placement, WebEngine, WebViewEvent, WebViewEvents, WebViewOptions,
    provide_web_engine,
};
use crate::error::WebViewError;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Opened {
    options: WebViewOptions,
    events: WebViewEvents,
    calls: RefCell<Vec<PageCommand>>,
    closed: Cell<bool>,
}

#[derive(Default)]
struct State {
    failure: Option<WebViewError>,
    pages: Vec<Rc<Opened>>,
}

#[derive(Clone, Default)]
pub struct FakeEngine(Rc<RefCell<State>>);

impl FakeEngine {
    pub fn new() -> Self {
        FakeEngine::default()
    }

    pub fn install(&self) -> impl FnOnce() + 'static {
        let engine = self.clone();
        move || provide_web_engine(engine)
    }

    pub fn failing(self, error: WebViewError) -> Self {
        self.0.borrow_mut().failure = Some(error);
        self
    }

    pub fn opened(&self) -> usize {
        self.0.borrow().pages.len()
    }

    fn page(&self, index: usize) -> Option<Rc<Opened>> {
        self.0.borrow().pages.get(index).cloned()
    }

    pub fn options(&self, page: usize) -> Option<WebViewOptions> {
        self.page(page).map(|page| page.options)
    }

    pub fn calls(&self, page: usize) -> Vec<PageCommand> {
        self.page(page)
            .map_or_default(|page| page.calls.borrow().clone())
    }

    pub fn take_calls(&self, page: usize) -> Vec<PageCommand> {
        self.page(page).map_or_default(|page| page.calls.take())
    }

    pub fn placement(&self, page: usize) -> Option<Placement> {
        self.page(page)?
            .calls
            .borrow()
            .iter()
            .rev()
            .find_map(|call| match call {
                PageCommand::Place(placed) => Some(*placed),
                _ => None,
            })
    }

    pub fn content(&self, page: usize) -> Option<WebContent> {
        self.page(page)?
            .calls
            .borrow()
            .iter()
            .rev()
            .find_map(|call| match call {
                PageCommand::Load(content) => Some(content.clone()),
                _ => None,
            })
    }

    pub fn closed(&self, page: usize) -> bool {
        self.page(page).is_some_and(|page| page.closed.get())
    }

    pub fn emit(&self, page: usize, event: WebViewEvent) {
        if let Some(page) = self.page(page) {
            page.events.send(event);
        }
    }
}

impl WebEngine for FakeEngine {
    fn open(
        &self,
        options: WebViewOptions,
        events: WebViewEvents,
    ) -> Result<Box<dyn EnginePage>, WebViewError> {
        let mut state = self.0.borrow_mut();
        if let Some(error) = state.failure.clone() {
            return Err(error);
        }
        let page = Rc::new(Opened {
            options,
            events,
            calls: RefCell::default(),
            closed: Cell::new(false),
        });
        state.pages.push(page.clone());
        Ok(Box::new(FakePage(page)))
    }
}

struct FakePage(Rc<Opened>);

impl EnginePage for FakePage {
    fn apply(&mut self, command: PageCommand) {
        self.0.calls.borrow_mut().push(command);
    }
}

impl Drop for FakePage {
    fn drop(&mut self) {
        self.0.closed.set(true);
    }
}

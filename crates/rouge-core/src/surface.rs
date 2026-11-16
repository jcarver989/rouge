use crate::control::dynamic;
use crate::style::{DelegateStyle, Styled};
use crate::tree::Kind;
use crate::view::{AnyView, BuildCx, Div, View, div};
use rouge_macros::component;
use rouge_reactive::{Signal, signal};
use rouge_render::{SurfaceConfig, SurfaceError, SurfaceId, SurfaceSource, SurfaceStatus};
use std::rc::Rc;

type Fallback = Rc<dyn Fn(SurfaceError) -> AnyView>;
type StatusSink = Box<dyn Fn(SurfaceStatus)>;

pub(crate) struct SurfaceSpec {
    pub id: SurfaceId,
    pub attach: Option<(SurfaceSource, StatusSink)>,
    pub placement: Option<SurfaceConfig>,
}

pub(crate) enum SurfaceOp {
    Attach(SurfaceSource, StatusSink),
    Place(SurfaceId, SurfaceConfig),
    Detach(SurfaceId),
}

pub struct Surface {
    div: Div,
    source: SurfaceSource,
    status: Option<Signal<SurfaceStatus>>,
    fallback: Option<Fallback>,
}

#[component]
pub fn surface(source: SurfaceSource) -> Surface {
    Surface {
        div: div(),
        source,
        status: None,
        fallback: None,
    }
}

impl Surface {
    pub fn status(mut self, status: Signal<SurfaceStatus>) -> Self {
        self.status = Some(status);
        self
    }

    pub fn fallback<T: View>(mut self, view: impl Fn(SurfaceError) -> T + 'static) -> Self {
        self.fallback = Some(Rc::new(move |error| view(error).into_any()));
        self
    }
}

impl DelegateStyle for Surface {
    fn style_target(&mut self) -> &mut (impl Styled + Default) {
        &mut self.div
    }
}

impl View for Surface {
    fn build(self, cx: &mut BuildCx<'_>) {
        let Surface {
            mut div,
            source,
            status,
            fallback,
        } = self;
        let id = source.id();
        let status = status.unwrap_or_else(|| signal(SurfaceStatus::Waiting));
        let sender = status.sender();
        div.spec.kind = Kind::Surface(Box::new(SurfaceSpec {
            id,
            attach: Some((source, Box::new(move |update| sender.set(update)))),
            placement: None,
        }));
        div.spec.layer.get_or_insert_default().surface = Some(id);
        if let Some(fallback) = fallback {
            div = div.child(dynamic(move || match status.get() {
                SurfaceStatus::Failed(error) => Some(fallback(error)),
                SurfaceStatus::Waiting | SurfaceStatus::Live => None,
            }));
        }
        div.build(cx);
    }
}

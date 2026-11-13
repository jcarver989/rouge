use crate::control::dynamic;
use crate::styled;
use crate::tree::ElementId;
use crate::view::{AnyView, BuildCx, Div, View, div};
use rouge_macros::component;
use rouge_reactive::{Signal, signal};
use rouge_render::{
    LayerCommand, Placement, SurfaceError, SurfaceId, SurfaceRegistrar, SurfaceSource,
    SurfaceStatus,
};
use std::rc::Rc;

type Fallback = Rc<dyn Fn(SurfaceError) -> AnyView>;
type StatusSink = Box<dyn Fn(SurfaceStatus)>;

pub(crate) struct SurfaceSpec {
    pub id: SurfaceId,
    pub attach: Option<(SurfaceSource, StatusSink)>,
    pub placement: Option<Placement>,
}

pub(crate) enum SurfaceOp {
    Attach(SurfaceSource, StatusSink),
    Place(SurfaceId, Placement),
    Detach(SurfaceId),
}

impl SurfaceOp {
    pub(crate) fn send(self, registrar: &SurfaceRegistrar) {
        match self {
            SurfaceOp::Attach(source, status) => registrar.attach(source, status),
            SurfaceOp::Place(id, placement) => registrar.place(id, placement),
            SurfaceOp::Detach(id) => registrar.detach(id),
        }
    }
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

styled!(Surface => div);

impl View for Surface {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let Surface {
            mut div,
            source,
            status,
            fallback,
        } = self;
        let id = source.id();
        let status = status.unwrap_or_else(|| signal(SurfaceStatus::Waiting));
        let sender = status.sender();
        div.spec.surface = Some(SurfaceSpec {
            id,
            attach: Some((source, Box::new(move |update| sender.set(update)))),
            placement: None,
        });
        div.spec
            .layer
            .get_or_insert_default()
            .commands
            .push(LayerCommand::Surface(Some(id)));
        if let Some(fallback) = fallback {
            div = div.child(dynamic(move || match status.get() {
                SurfaceStatus::Failed(error) => Some(fallback(error)),
                SurfaceStatus::Waiting | SurfaceStatus::Live => None,
            }));
        }
        div.build(cx)
    }
}

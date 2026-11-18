use super::model::Todos;
use super::tokens::{
    CARD_WIDTH, READING_FADE, RING_ECHO, RING_ENTER, RING_EXIT, RING_GRAPHITE, RING_INK,
    RING_LIMIT, RING_ORIGIN, RING_PITCH, RING_RADIUS, WAVE_AMPLITUDE, WAVE_DAMPING, WAVE_LENGTH,
    WAVE_SECONDS, WAVE_SPEED,
};
use rouge::prelude::*;
use rouge::widgets::tokens::{HAIRLINE, Hold, MotionScale, Pace, Radius, Space};

#[derive(Clone, Copy, Default)]
pub(crate) enum Touch {
    #[default]
    Unused,
    Card,
    Press(Point),
}

#[component]
pub(crate) fn ring_field(todos: Todos, touch: Stored<Touch>, card: Signal<Rect>) -> impl View {
    let keys = todos.items.keys();
    let order =
        memo(move || keys.with(|ids| ids.iter().take(RING_LIMIT).copied().collect::<Vec<_>>()));
    let reading = move || {
        let card = card.get();
        Reading {
            floor: card.origin.y + card.size.height,
        }
    };
    view! {
        <div absolute inset=0.0 pass_through overflow_hidden bg=Role::Canvas>
            <div absolute inset=0.0 flex justify_center px=Space::X4 effect=reading transition=Pace::Layout>
                <div relative w_full max_w=CARD_WIDTH h_full>
                    <each items=order key={|id| *id} let:id
                          enter=(RING_ENTER, Pace::Layout) exit=(RING_EXIT, Pace::Exit)>
                        <ring todos order id={id.get_untracked()}/>
                    </each>
                </div>
            </div>
            <waves todos touch card/>
        </div>
    }
}

#[component]
fn ring(todos: Todos, order: Memo<Vec<u64>>, id: u64) -> impl View {
    let done = todos.items.select(id, |todo| todo.done);
    let slot = stored(0.0);
    let radius = move || {
        let found = order.with(|ids| {
            ids.iter()
                .zip(0u16..)
                .find(|(ring, _)| **ring == id)
                .map(|(_, slot)| slot)
        });
        if let Some(found) = found {
            slot.set(f32::from(found));
        }
        RING_RADIUS - slot.get() * RING_PITCH
    };
    let echoes = [1.0, 2.0, 3.0].map(|step| {
        view! {
            <div absolute inset={step * RING_ECHO} rounded=Radius::Full border=HAIRLINE border_color=Role::Border/>
        }
    });
    view! {
        <div absolute left={move || -(RING_ORIGIN.x + radius())} bottom={move || -(RING_ORIGIN.y + radius())}
             size={move || 2.0 * radius()} layout_transition={LayoutTransition::new(Pace::Layout).size()}>
            {..echoes}
            <div absolute inset=0.0 rounded=Radius::Full border=HAIRLINE border_color=Role::TextSecondary
                 opacity={done.pick(0.0, RING_GRAPHITE)} transition=Pace::Layout/>
            <div absolute inset=0.0 rounded=Radius::Full border=RING_INK border_color=Role::Accent
                 opacity={done.pick(1.0, 0.0)} transition=Pace::Layout/>
        </div>
    }
}

#[component]
fn waves(todos: Todos, touch: Stored<Touch>, card: Signal<Rect>) -> impl View {
    let theme = Theme::current();
    let wave = signal(None::<(u64, Point)>);
    let clock = motion(0.0);
    if MotionScale::current() == MotionScale::Full {
        watch(
            move || (todos.items.len(), todos.remaining.get()),
            move |_, before| {
                let origin = match touch.get() {
                    Touch::Press(origin) if before.is_some() => origin,
                    Touch::Card if before.is_some() => {
                        let card = card.get_untracked();
                        Point::new(
                            card.origin.x + Space::X8.px(),
                            card.origin.y + Space::X8.px(),
                        )
                    }
                    _ => return,
                };
                touch.set(Touch::Card);
                let count = wave.with_untracked(|wave| wave.map_or(0, |(count, _)| count + 1));
                wave.set(Some((count, origin)));
                clock.set(0.0);
                let spent = clock.animate_to(1.0, Hold::seconds(WAVE_SECONDS));
                spawn(async move {
                    if spent.await {
                        wave.set(None);
                    }
                });
            },
        );
    }
    view! {
        <each items={move || wave.get().into_iter().collect::<Vec<_>>()} key={|(count, _)| *count} let:wave>
            {
                let (_, origin) = wave.get_untracked();
                let bend = move || {
                    let palette = theme.get();
                    Wave {
                        origin,
                        paper: palette.canvas,
                        ink: palette.accent,
                    }
                };
                view! { <div absolute inset=0.0 pass_through backdrop=bend test_id="wave"/> }
            }
        </each>
    }
}

struct Reading {
    floor: f32,
}

impl ShaderEffect for Reading {
    const WGSL: &'static str = include_str!("rings.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[..4].copy_from_slice(&[CARD_WIDTH, Space::X4.px(), self.floor, READING_FADE]);
    }
}

struct Wave {
    origin: Point,
    paper: Color,
    ink: Color,
}

impl ShaderEffect for Wave {
    const WGSL: &'static str = include_str!("wave.wgsl");
    const INPUTS: Inputs = Inputs::BACKDROP;
    const CLOCK: Option<usize> = Some(0);

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[1..4].copy_from_slice(&[WAVE_SECONDS, self.origin.x, self.origin.y]);
        uniforms[4..8].copy_from_slice(&self.paper.to_array());
        uniforms[8..12].copy_from_slice(&self.ink.to_array());
        uniforms[12..].copy_from_slice(&[WAVE_SPEED, WAVE_AMPLITUDE, WAVE_DAMPING, WAVE_LENGTH]);
    }

    fn backdrop_reach(&self) -> Option<f32> {
        Some(WAVE_AMPLITUDE)
    }
}

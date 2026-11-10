use rouge::prelude::*;
use rouge::render::Effect;

const BASE: u32 = 0x14151f;
const SURFACE: u32 = 0x232536;
const TEXT: u32 = 0xe6e9f5;
const SUBTLE: u32 = 0x9aa0b8;
const EASE: Tween = Tween::ms(260.0).ease(Ease::cubic_bezier(0.2, 0.8, 0.2, 1.0));

#[derive(Clone, Copy)]
struct Ripple {
    frequency: f32,
    strength: f32,
}

impl Effect for Ripple {
    const WGSL: &'static str = include_str!("ripple.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;

    fn uniforms(&self) -> Vec<f32> {
        vec![self.frequency, self.strength]
    }

    fn margin(&self) -> f32 {
        self.strength.abs()
    }
}

fn artwork() -> Div {
    let stripe = |color: u32| div().flex_1().bg(color);
    div()
        .w(120.0)
        .h(90.0)
        .flex()
        .rounded(10.0)
        .overflow_hidden()
        .children([0xf5507a, 0xffc247, 0x38d39f, 0x3d7dff].map(stripe))
}

fn tile(label: &'static str, art: impl View) -> impl View {
    div()
        .flex()
        .col()
        .items_center()
        .gap(10.0)
        .p(14.0)
        .rounded(14.0)
        .bg(SURFACE)
        .child(art)
        .child(text(label).text_size(12.0).weight(600.0).color(SUBTLE))
}

fn blended() -> impl View {
    let disc = |x: f32, color: u32| {
        div()
            .absolute()
            .left(x)
            .top(15.0)
            .size(60.0)
            .rounded(30.0)
            .bg(color)
    };
    div()
        .relative()
        .w(120.0)
        .h(90.0)
        .rounded(10.0)
        .bg(0xdfe3f0)
        .child(disc(10.0, 0xf5507a).blend(BlendMode::Multiply))
        .child(disc(50.0, 0x3d7dff).blend(BlendMode::Multiply))
}

fn chip() -> impl View {
    let (x, y) = (motion(0.0), motion(0.0));
    let origin = signal((0.0f32, 0.0f32));
    let drag = move |drag: &DragEvent| match drag.phase {
        DragPhase::Start => origin.set((x.get(), y.get())),
        DragPhase::Move => {
            let (left, top) = origin.get_untracked();
            x.set(left + drag.translation.x);
            y.set(top + drag.translation.y);
        }
        DragPhase::End => {
            x.decay(drag.velocity.x, Decay::new().rate(5.0));
            y.decay(drag.velocity.y, Decay::new().rate(5.0));
        }
    };
    div()
        .absolute()
        .left(250.0)
        .top(150.0)
        .px(22.0)
        .py(14.0)
        .rounded(22.0)
        .backdrop(Glass::new().blur(6.0).edge_width(12.0))
        .translate_x(x)
        .translate_y(y)
        .on_drag(drag)
        .child(
            text("Drag and fling me")
                .text_size(14.0)
                .weight(600.0)
                .color(Color::WHITE),
        )
}

pub(crate) fn effects_demo(active: Signal<bool>) -> impl View {
    let on = move || active.get();
    let blur = move || Blur::new(if on() { 6.0 } else { 0.0 });
    let color = move || {
        if on() {
            ColorMatrix::grayscale()
        } else {
            ColorMatrix::saturate(1.5)
        }
    };
    let ripple = move || Ripple {
        frequency: 0.35,
        strength: if on() { 5.0 } else { 0.0 },
    };
    let fade = Mask::linear_gradient(Point::new(0.0, 0.2), Point::new(0.0, 1.0));
    let toggle = div()
        .px(18.0)
        .py(10.0)
        .bg(move || if on() { 0x3d7dff } else { 0x45475a })
        .rounded(move || if on() { 19.0 } else { 6.0 })
        .hover(|s| s.bg(0x5b8cff))
        .transition(EASE)
        .on_click(move |_| active.update(|active| *active = !*active))
        .child(
            text("Toggle effects")
                .text_size(13.0)
                .weight(600.0)
                .color(TEXT),
        );
    let tiles = div()
        .flex()
        .gap(14.0)
        .child(tile("Blur", artwork().effect(blur).transition(EASE)))
        .child(tile(
            "Color matrix",
            artwork().effect(color).transition(EASE),
        ))
        .child(tile("Mask", artwork().mask(fade)))
        .child(tile("Multiply", blended()))
        .child(tile(
            "Custom WGSL",
            artwork().effect(ripple).transition(EASE),
        ));
    div()
        .size_full()
        .relative()
        .flex()
        .col()
        .items_start()
        .gap(18.0)
        .p(24.0)
        .bg(BASE)
        .child(tiles)
        .child(toggle)
        .child(chip())
}

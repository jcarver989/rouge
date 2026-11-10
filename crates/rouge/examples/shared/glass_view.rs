use rouge::prelude::*;

fn blob(x: f32, y: f32, size: f32, color: u32) -> impl View {
    div()
        .absolute()
        .left(x)
        .top(y)
        .size(size)
        .rounded(size / 2.0)
        .bg(color)
}

fn pill(label: &'static str) -> impl View {
    div()
        .px(14.0)
        .py(7.0)
        .rounded(16.0)
        .backdrop(Glass::new().blur(2.0).edge_width(8.0))
        .child(
            text(label)
                .text_size(13.0)
                .weight(600.0)
                .color(Color::WHITE),
        )
}

pub(crate) fn glass(lifted: Signal<bool>) -> impl View {
    div()
        .size_full()
        .bg(0x10111a)
        .child(blob(40.0, 30.0, 220.0, 0xf5507a))
        .child(blob(300.0, 120.0, 260.0, 0x3d7dff))
        .child(blob(150.0, 210.0, 180.0, 0xffc247))
        .child(blob(430.0, 10.0, 120.0, 0x38d39f))
        .child(
            div().absolute().left(36.0).top(150.0).child(
                text("Behind the glass")
                    .text_size(44.0)
                    .bold()
                    .color(Color::WHITE),
            ),
        )
        .child(
            div()
                .absolute()
                .left(110.0)
                .top(70.0)
                .w(380.0)
                .h(230.0)
                .flex()
                .col()
                .justify_between()
                .p(22.0)
                .rounded(36.0)
                .backdrop(Glass::new().blur(3.0).edge_width(22.0))
                .translate_y(move || if lifted.get() { -24.0 } else { 0.0 })
                .scale(move || if lifted.get() { 1.04 } else { 1.0 })
                .transition(Spring::bouncy())
                .on_click(move |_| lifted.update(|l| *l = !*l))
                .child(
                    div()
                        .flex()
                        .col()
                        .gap(4.0)
                        .child(
                            text("Liquid glass")
                                .text_size(24.0)
                                .bold()
                                .color(Color::WHITE),
                        )
                        .child(
                            text("Blur, refraction and a specular rim, composited on the GPU.")
                                .text_size(13.0)
                                .color(Color::WHITE.with_alpha(0.8)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(8.0)
                        .child(pill("Click me"))
                        .child(pill("Frosted")),
                ),
        )
}

use super::palette::{ACCENT, BASE, MUTED, RED, SURFACE};
use rouge::prelude::*;

pub(crate) fn counter() -> impl View {
    provide_theme(Palette::dark());
    let count = signal(0i32);
    let parity = memo(move || if count.get() % 2 == 0 { "even" } else { "odd" });

    view! {
        <div size_full flex col items_center justify_center gap=20.0 bg=BASE>
            <div flex col items_center gap=4.0 px=40.0 py=24.0 rounded=16.0 bg=SURFACE
                 shadow=BoxShadow::lg()>
                <text text_size=56.0 bold color={move || if count.get() < 0 { RED } else { ACCENT }}>
                    "{count}"
                </text>
                <text color=MUTED>"is {parity}"</text>
            </div>
            <div flex gap=8.0>
                <button label="- 1" on_click={move || count.update(|c| *c -= 1)}/>
                <button label="Reset" on_click={move || count.set(0)}/>
                <button label="+ 1" on_click={move || count.update(|c| *c += 1)}/>
            </div>
            <show when={move || count.get() >= 5}>
                <text text_size=13.0 italic color=MUTED>"That's a lot of clicks."</text>
            </show>
        </div>
    }
}

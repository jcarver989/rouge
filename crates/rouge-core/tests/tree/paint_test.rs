mod common;

use common::{Random, edit};
use rouge_core::testing::{RED, TestResult, TestUi, TestUiBuilder};
use rouge_core::{Color, Palette, Role, StyleDelta, Styled, Theme, div, each, scroll, text};
use rouge_reactive::{ReadSignal, Runtime, Signal, signal};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Rows {
    Flat,
    Layered,
    Wrapped,
}

#[test]
fn edited_scroll_content_matches_a_fresh_frame() -> TestResult {
    let _rt = Runtime::new().enter();
    let ids = signal((0..128).collect::<Vec<u32>>());
    let hot = signal(false);
    let width = signal(49.0);
    for rows in [Rows::Flat, Rows::Layered, Rows::Wrapped] {
        ids.set((0..128).collect());
        hot.set(false);
        width.set(49.0);
        let mut ui = table(ids, hot, width, rows);
        let initial = ui.draw()?.instance_bytes_uploaded;
        hot.set(true);
        ui.advance(0.0);
        let uploaded = ui.draw()?.instance_bytes_uploaded;
        assert!(uploaded > 0 && uploaded * 2 < initial, "{rows:?}");
        for step in 0..5 {
            match step {
                1 => ids.update(|ids| ids.swap(2, 124)),
                2 => ids.update(|ids| {
                    ids.remove(1);
                }),
                3 => width.set(71.0),
                4 => ids.update(|ids| ids.truncate(50)),
                _ => {}
            }
            ui.advance(0.0);
            let mut fresh = table(ids, hot, width, rows);
            assert!(
                ui.render()?.pixels == fresh.render()?.pixels,
                "step {step}, {rows:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn cached_rows_match_a_fresh_frame_after_any_edit() -> TestResult {
    let _rt = Runtime::new().enter();
    let ids = signal(Vec::<u32>::new());
    let tall = signal(0u32);
    for wrapped in [false, true] {
        ids.set((0..70).collect());
        tall.set(0);
        let mut ui = grid(ids, tall, wrapped);
        let (mut random, mut next) = (Random(11), 1000);
        for step in 0..120 {
            ids.update(|ids| edit(ids, &mut next, &mut random));
            if random.below(3) == 0 {
                let pick = random.below(ids.with_untracked(Vec::len).max(1));
                tall.set(ids.with_untracked(|ids| ids.get(pick).copied().unwrap_or(0)));
            }
            ui.advance(0.0);
            let mut fresh = grid(ids, tall, wrapped);
            assert!(
                ui.render()?.pixels == fresh.render()?.pixels,
                "step {step}, wrapped={wrapped}, {:?}",
                ids.get_untracked()
            );
        }
    }
    Ok(())
}

#[test]
fn cached_rows_follow_text_styles_inherited_from_outside_them() -> TestResult {
    let _rt = Runtime::new().enter();
    let warm = signal(false);
    let build = move || {
        TestUiBuilder::default().size(96.0, 200.0).build(move || {
            let rows = (0..80).map(|row| div().h(12.0).child(text(format!("row {row}"))));
            let color = move || if warm.get() { Color::RED } else { Color::BLUE };
            div()
                .size_full()
                .text_color(color)
                .child(scroll().size_full().children(rows))
        })
    };
    let mut ui = build();
    let cold = ui.render()?.pixels;
    warm.set(true);
    ui.advance(0.0);
    let shown = ui.render()?.pixels;
    assert!(shown != cold, "the rows draw their text");
    assert!(shown == build().render()?.pixels);
    Ok(())
}

#[test]
fn content_hidden_during_a_theme_change_repaints_when_shown() -> TestResult {
    let _rt = Runtime::new().enter();
    let hidden = signal(false);
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| {
        let rows = (0..128).map(|_| div().w(80.0).h(1.0).bg(Role::Accent));
        let list = scroll().size_full().children(rows);
        (
            div()
                .size_full()
                .when(move || hidden.get(), StyleDelta::invisible)
                .child(list),
            Theme::current(),
        )
    });
    assert_ne!(ui.pixel(10, 10)?, RED);
    hidden.set(true);
    ui.advance(0.0);
    theme.set(Palette {
        accent: Color::RED,
        ..theme.get_untracked()
    });
    ui.advance(0.0);
    hidden.set(false);
    ui.advance(0.0);
    assert_eq!(ui.pixel(10, 10)?, RED);
    Ok(())
}

fn table(ids: Signal<Vec<u32>>, hot: Signal<bool>, width: Signal<f32>, rows: Rows) -> TestUi {
    TestUiBuilder::default()
        .size(96.0, 270.0)
        .scale(1.5)
        .build(move || {
            let each = each(
                move || ids.get(),
                |id| *id,
                move |id: ReadSignal<u32>| {
                    let id = id.get_untracked();
                    let inner = div().w(88.0).h(8.0).bg(move || {
                        if id == 2 && hot.get() {
                            Color::RED
                        } else {
                            Color::hsl(id as f32 * 17.0, 0.7, 0.5)
                        }
                    });
                    div().w(move || width.get()).h(2.0).overflow_hidden().child(
                        if rows == Rows::Layered && id == 5 {
                            inner.layer()
                        } else {
                            inner
                        },
                    )
                },
            );
            if rows == Rows::Wrapped {
                scroll().size_full().child(div().child(each))
            } else {
                scroll().size_full().child(each)
            }
        })
}

fn grid(ids: Signal<Vec<u32>>, tall: Signal<u32>, wrapped: bool) -> TestUi {
    TestUiBuilder::default()
        .size(96.0, 300.0)
        .scale(1.5)
        .build(move || {
            let each = each(
                move || ids.get(),
                |id| *id,
                move |id: ReadSignal<u32>| {
                    let id = id.get_untracked();
                    let mark = div()
                        .w(move || 10.0 + (tall.get() % 7) as f32)
                        .h(1.0)
                        .bg(Color::RED);
                    let cells = if id.is_multiple_of(11) { 70 } else { 0 };
                    let cells = (0..cells)
                        .map(|cell| div().size(0.5).bg(Color::hsl(cell as f32 * 5.0, 0.9, 0.4)));
                    div()
                        .flex()
                        .w(80.0)
                        .h(move || if tall.get() == id { 5.0 } else { 2.0 })
                        .bg(Color::hsl(id as f32 * 17.0, 0.7, 0.5))
                        .child(text(format!("{id}")))
                        .child(if id.is_multiple_of(7) {
                            mark.layer()
                        } else {
                            mark
                        })
                        .children(cells)
                },
            );
            if wrapped {
                scroll().size_full().child(div().p(1.0).child(each))
            } else {
                scroll().size_full().child(each)
            }
        })
}

#[path = "../common/random.rs"]
mod random;

use random::Random;
use rouge_core::testing::{RED, TestResult, TestUi, TestUiBuilder, WHITE};
use rouge_core::{
    AnyView, Color, Corners, Palette, Role, Size, StyleDelta, Styled, Theme, UiPalette, View, div,
    each, scroll, text,
};
use rouge_reactive::{ReadSignal, Runtime, Signal, batch, signal};

const SIZE: Size = Size::new(160.0, 240.0);

#[test]
fn every_edit_paints_what_a_fresh_window_would() -> TestResult {
    let _rt = Runtime::new().enter();
    for (scale, steps) in [(1.0, 40), (1.5, 30), (2.0, 20)] {
        let mut random = Random(11);
        let mut next = 0;
        let mut first = Vec::new();
        for _ in 0..150 {
            first.push(Row::random(&mut next, &mut random));
        }
        let page = Page::new(first);
        let mut ui = window(scale, page);
        for step in 0..steps {
            batch(|| page.edit(&mut next, &mut random));
            ui.advance(0.0);
            let drawn = ui.render()?;
            let mut fresh = window(scale, page.copy());
            let expected = fresh.render()?;
            assert!(
                drawn.pixels == expected.pixels,
                "step {step} at scale {scale}"
            );
        }
    }
    Ok(())
}

#[test]
fn changing_one_row_paints_and_uploads_only_its_block() -> TestResult {
    let _rt = Runtime::new().enter();
    let hot = signal(false);
    let mut ui = list(1000, move |index| {
        let row = div().h(10.0).w_full();
        if index == 3 {
            row.bg(move || if hot.get() { Color::RED } else { Color::BLUE })
        } else {
            row.bg(Color::BLACK)
        }
    });
    let whole = ui.draw()?;
    assert!(whole.commands_visited > 1);
    let quiet = ui.draw()?;
    assert_eq!((quiet.commands_visited, quiet.records_built), (0, 0));
    let before = ui.stats();
    hot.set(true);
    ui.advance(0.0);
    let drawn = ui.draw()?;
    let work = ui.stats() - before;
    assert!(drawn.damaged > 0 && drawn.instance_bytes_uploaded > 0);
    assert_eq!((work.layouts, work.block_paints), (0, 1));
    assert!(drawn.instance_bytes_uploaded * 20 < whole.instance_bytes_uploaded);
    Ok(())
}

#[test]
fn rows_below_a_removed_row_move_without_being_painted() -> TestResult {
    let _rt = Runtime::new().enter();
    let items = signal((0..1000u32).collect::<Vec<_>>());
    let mut ui = rows(items);
    ui.draw()?;
    let before = ui.stats();
    items.update(|items| {
        items.remove(2);
    });
    ui.advance(0.0);
    ui.draw()?;
    let work = ui.stats() - before;
    assert_eq!((work.layouts, work.block_paints), (1, 1));
    Ok(())
}

#[test]
fn swapping_two_rows_paints_their_two_blocks() -> TestResult {
    let _rt = Runtime::new().enter();
    let items = signal((0..1000u32).collect::<Vec<_>>());
    let mut ui = rows(items);
    ui.draw()?;
    let before = ui.stats();
    items.update(|items| items.swap(1, 998));
    ui.advance(0.0);
    ui.draw()?;
    assert_eq!((ui.stats() - before).block_paints, 2);
    Ok(())
}

#[test]
fn appending_one_row_at_a_time_never_paints_many_blocks() -> TestResult {
    let _rt = Runtime::new().enter();
    let items = signal((0..1000u32).collect::<Vec<_>>());
    let mut ui = rows(items);
    ui.draw()?;
    for next in 1000..1100 {
        let before = ui.stats();
        items.update(|items| items.push(next));
        ui.advance(0.0);
        ui.draw()?;
        let painted = (ui.stats() - before).block_paints;
        assert!(painted <= 2, "row {next} painted {painted} blocks");
    }
    Ok(())
}

#[test]
fn a_theme_switch_paints_every_block() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| {
        let rows = (0..640).map(|index| text(format!("{index}")));
        let view = div().size_full().child(scroll().size_full().children(rows));
        (view, Theme::current())
    });
    ui.draw()?;
    let before = ui.stats();
    theme.set(UiPalette::dark());
    ui.advance(0.0);
    ui.draw()?;
    assert_eq!((ui.stats() - before).block_paints, 20);
    Ok(())
}

#[test]
fn a_layer_hidden_during_a_theme_change_repaints_its_blocks_when_shown() -> TestResult {
    let _rt = Runtime::new().enter();
    let hidden = signal(false);
    let (mut ui, theme) = TestUiBuilder::default().build_with(|| {
        let rows = (0..128).map(|_| div().w(80.0).h(1.0).bg(Role::Accent));
        let list = div()
            .layer()
            .size_full()
            .when(move || hidden.get(), StyleDelta::invisible);
        (
            div().size_full().child(list.children(rows)),
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

#[test]
fn rows_stretched_by_their_list_are_painted_again() -> TestResult {
    let _rt = Runtime::new().enter();
    let width = signal(50.0);
    let mut ui = TestUi::canvas(move || {
        let rows = (0..100).map(|_| div().h(1.0).w_full().bg(Color::RED));
        let list = div().w(move || width.get()).children(rows);
        div().size_full().child(list)
    });
    assert_eq!(ui.pixel(80, 40)?, WHITE);
    width.set(100.0);
    ui.advance(0.0);
    assert_eq!(ui.pixel(80, 40)?, RED);
    Ok(())
}

#[test]
fn a_block_list_hidden_and_shown_again_paints_the_same() -> TestResult {
    let _rt = Runtime::new().enter();
    let hidden = signal(false);
    let mut ui = TestUi::new(move || {
        let rows = (0..200).map(|index| div().h(1.0).bg(shade(index)));
        div()
            .size_full()
            .when(move || hidden.get(), StyleDelta::invisible)
            .child(div().w_full().children(rows))
    });
    let shown = ui.render()?;
    hidden.set(true);
    ui.advance(0.0);
    assert!(ui.render()?.pixels != shown.pixels);
    hidden.set(false);
    ui.advance(0.0);
    assert!(ui.render()?.pixels == shown.pixels);
    Ok(())
}

#[test]
fn a_layer_appears_when_it_is_shown_with_or_without_blocks_around_it() -> TestResult {
    for count in [50, 100] {
        shows_a_layer_among(count)?;
    }
    Ok(())
}

#[derive(Clone, PartialEq)]
struct Row {
    id: u32,
    kind: u32,
    height: f32,
    color: usize,
    hidden: bool,
    boxes: usize,
}

impl Row {
    fn random(next: &mut u32, random: &mut Random) -> Self {
        *next += 1;
        Row {
            id: *next,
            kind: random.below(9) as u32,
            height: 1.0 + random.below(3) as f32,
            color: random.below(1000),
            hidden: random.below(12) == 0,
            boxes: if random.below(40) == 0 { 70 } else { 2 },
        }
    }
}

#[derive(Clone, Copy)]
struct Page {
    rows: Signal<Vec<Row>>,
    header: Signal<f32>,
    clipped: Signal<f32>,
    width: Signal<f32>,
    padding: Signal<f32>,
}

impl Page {
    fn new(rows: Vec<Row>) -> Self {
        Page {
            rows: signal(rows),
            header: signal(4.0),
            clipped: signal(120.0),
            width: signal(100.0),
            padding: signal(0.0),
        }
    }

    fn copy(self) -> Self {
        Page {
            rows: signal(self.rows.get_untracked()),
            header: signal(self.header.get_untracked()),
            clipped: signal(self.clipped.get_untracked()),
            width: signal(self.width.get_untracked()),
            padding: signal(self.padding.get_untracked()),
        }
    }

    fn edit(self, next: &mut u32, random: &mut Random) {
        for _ in 0..=random.below(2) {
            match random.below(16) {
                0 => self.header.set(random.below(10) as f32),
                1 => self.clipped.set(60.0 + random.below(100) as f32),
                2 => self.width.set(60.0 + random.below(60) as f32),
                3 => self.padding.set(random.below(8) as f32),
                _ => self.rows.update(|rows| change(rows, next, random)),
            }
        }
    }

    fn view(self) -> impl View {
        let list = move || each(move || self.rows.get(), |row| row.id, show_row);
        let clipped = div()
            .overflow_hidden()
            .h(move || self.clipped.get())
            .child(div().w(move || self.width.get()).child(list()));
        let scrolled = scroll()
            .flex_1()
            .min_h(0.0)
            .pt(move || self.padding.get())
            .bg(Color::WHITE)
            .child(list());
        div()
            .flex()
            .col()
            .size_full()
            .bg(Color::WHITE)
            .child(div().h(move || self.header.get()).bg(Color::GREEN))
            .child(clipped)
            .child(scrolled)
    }
}

fn change(rows: &mut Vec<Row>, next: &mut u32, random: &mut Random) {
    let place = random.below(rows.len() + 1);
    let picked = place.min(rows.len().saturating_sub(1));
    match random.below(10) {
        0 if rows.len() > 2 => {
            let other = random.below(rows.len());
            rows.swap(picked, other);
        }
        1 if rows.len() > 100 => {
            rows.drain(picked..(picked + random.below(40)).min(rows.len()));
        }
        2 => {
            for _ in 0..random.below(40) {
                rows.insert(place, Row::random(next, random));
            }
        }
        3 if !rows.is_empty() => {
            rows.remove(picked);
        }
        4 => rows.insert(place, Row::random(next, random)),
        5 if !rows.is_empty() => rows[picked].height = 1.0 + random.below(4) as f32,
        6 if !rows.is_empty() => rows[picked].hidden ^= true,
        7 if !rows.is_empty() => rows[picked].boxes = 66 + random.below(8),
        8 => rows.rotate_left(place),
        _ if !rows.is_empty() => rows[picked].color = random.below(1000),
        _ => {}
    }
}

fn show_row(row: ReadSignal<Row>) -> AnyView {
    let color = move || shade(row.with(|row| row.color));
    let kind = row.get_untracked().kind;
    let base = div()
        .w_full()
        .flex_shrink(0.0)
        .h(move || row.with(|row| row.height));
    let hidden = move || row.with(|row| row.hidden);
    match kind {
        0 => base
            .bg(color)
            .opacity(0.5)
            .when(hidden, StyleDelta::invisible)
            .into_any(),
        1 => base
            .layer()
            .bg(color)
            .when(hidden, StyleDelta::invisible)
            .child(div().size(2.0).bg(Color::BLACK))
            .into_any(),
        2 => base
            .overflow_hidden()
            .child(div().w(50.0).h(20.0).bg(color))
            .into_any(),
        3 => base
            .child(text(move || row.with(|row| format!("{}", row.color))).text_size(6.0))
            .into_any(),
        4 => base
            .rounded(Corners::all(2.0))
            .border(1.0)
            .border_color(color)
            .into_any(),
        5 => {
            let boxes = each(
                move || (0..row.with(|row| row.boxes)).collect::<Vec<usize>>(),
                |index| *index,
                move |index: ReadSignal<usize>| {
                    let index = index.get_untracked();
                    let color = move || shade(index * 37 + row.with(|row| row.color));
                    div().size(2.0).bg(color)
                },
            );
            base.flex().child(boxes).into_any()
        }
        6 => base
            .overflow_hidden()
            .child(div().layer().w(40.0).h(20.0).bg(color))
            .into_any(),
        7 => base
            .child(
                div()
                    .layer()
                    .size(3.0)
                    .bg(color)
                    .when(hidden, StyleDelta::invisible),
            )
            .into_any(),
        _ => base
            .bg(color)
            .when(hidden, StyleDelta::invisible)
            .into_any(),
    }
}

fn shows_a_layer_among(count: usize) -> TestResult {
    let _rt = Runtime::new().enter();
    let hidden = signal(true);
    let mut ui = TestUi::new(move || {
        let rows = (0..count).map(move |index| {
            let row = div().h(1.0).w_full().bg(Color::BLACK);
            if index == 40 {
                let badge = div().layer().size_full().bg(Color::RED);
                row.child(badge.when(move || hidden.get(), StyleDelta::invisible))
            } else {
                row
            }
        });
        div().size_full().children(rows)
    });
    assert_ne!(ui.pixel(5, 40)?, RED);
    hidden.set(false);
    ui.advance(0.0);
    assert_eq!(ui.pixel(5, 40)?, RED);
    hidden.set(true);
    ui.advance(0.0);
    assert_ne!(ui.pixel(5, 40)?, RED);
    Ok(())
}

fn shade(seed: usize) -> Color {
    Color::hsl((seed * 47 % 360) as f32, 0.7, 0.5)
}

fn window(scale: f32, page: Page) -> TestUi {
    TestUiBuilder::default()
        .size(SIZE.width, SIZE.height)
        .scale(scale)
        .clear(Color::BLACK)
        .build(move || page.view())
}

fn list<T: View>(count: usize, row: impl Fn(usize) -> T) -> TestUi {
    TestUi::canvas(|| {
        let rows = (0..count).map(row);
        div().size_full().child(scroll().size_full().children(rows))
    })
}

fn rows(items: Signal<Vec<u32>>) -> TestUi {
    TestUi::canvas(move || {
        let row = |id: ReadSignal<u32>| div().h(10.0).bg(shade(id.get_untracked() as usize));
        let rows = each(move || items.get(), |id| *id, row);
        div().size_full().child(scroll().size_full().child(rows))
    })
}

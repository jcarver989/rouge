use rouge::prelude::*;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Density {
    Compact,
    Comfortable,
    Spacious,
}

struct Save;

const BELL: &str = "M12 3 C8 3 6 6 6 10 V14 L4 17 H20 L18 14 V10 C18 6 16 3 12 3 Z M10 19 H14";

fn row(label: &'static str, control: impl View) -> impl View {
    let theme = use_theme();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(16.0)
        .py(8.0)
        .child(text(label).color(theme.text).text_size(theme.text_size))
        .child(control)
}

fn section(title: &'static str) -> Div {
    let theme = use_theme();
    div()
        .flex()
        .col()
        .p(16.0)
        .rounded(12.0)
        .bg(theme.surface)
        .border(1.0)
        .border_color(theme.border)
        .child(text(title).color(theme.text_muted).text_size(12.0).bold())
}

fn general(name: Signal<String>, notes: Signal<String>, saved: Signal<u32>) -> impl View {
    let theme = use_theme();
    let (notify, sounds) = (signal(true), signal(false));
    let volume = signal(60.0f32);
    let language = signal("en");
    let languages = [("en", "English"), ("fr", "Français"), ("ja", "日本語")];
    let options = languages.map(|(code, label)| (code, label.to_owned()));
    let bell = icon(Path::svg(BELL))
        .stroke(1.8)
        .color(theme.text_muted)
        .size(18.0);
    div()
        .flex()
        .col()
        .gap(12.0)
        .p(16.0)
        .child(
            section("PROFILE")
                .child(row(
                    "Display name",
                    text_input(name).placeholder("Your name").w(220.0),
                ))
                .child(row("Language", select(language, options).w(220.0)))
                .child(text_area(notes).placeholder("Notes").w_full())
                .child(
                    text(move || format!("Saved {} times (⌘S)", saved.get()))
                        .color(theme.text_muted)
                        .text_size(12.0),
                ),
        )
        .child(
            section("NOTIFICATIONS")
                .child(row(
                    "Allow notifications",
                    tooltip(toggle(notify), "Show banners and badges"),
                ))
                .child(row("Play sounds", checkbox(sounds).label("Enabled")))
                .child(row(
                    "Volume",
                    div()
                        .flex()
                        .items_center()
                        .gap(8.0)
                        .child(bell)
                        .child(slider(volume).range(0.0, 100.0).step(5.0).w(180.0)),
                ))
                .child(progress(move || volume.get() / 100.0).w_full()),
        )
}

fn appearance(dark: Signal<bool>) -> impl View {
    let density = signal(Density::Comfortable);
    let accent = signal(0usize);
    let swatches = [0x0a6cff, 0xd6336c, 0x2f9e44, 0xf08c00];
    let theme = use_theme();
    effect(move || {
        let base = if dark.get() {
            Palette::dark()
        } else {
            Palette::light()
        };
        let spacing = match density.get() {
            Density::Compact => 4.0,
            Density::Comfortable => 8.0,
            Density::Spacious => 12.0,
        };
        theme.set(Palette {
            accent: Color::rgb(swatches[accent.get()]),
            spacing,
            ..base
        });
    });
    let swatch = move |index: usize| {
        div()
            .size(22.0)
            .rounded(11.0)
            .bg(swatches[index])
            .border(2.0)
            .border_color(move || {
                if accent.get() == index {
                    theme.text.get()
                } else {
                    Color::TRANSPARENT
                }
            })
            .cursor(Cursor::Pointer)
            .on_click(move |_| accent.set(index))
    };
    div().flex().col().gap(12.0).p(16.0).child(
        section("THEME")
            .child(row("Dark mode", toggle(dark)))
            .child(row(
                "Accent",
                div()
                    .flex()
                    .gap(8.0)
                    .children((0..swatches.len()).map(swatch)),
            ))
            .child(row(
                "Density",
                div()
                    .flex()
                    .gap(12.0)
                    .child(radio(density, Density::Compact).label("Compact"))
                    .child(radio(density, Density::Comfortable).label("Comfortable"))
                    .child(radio(density, Density::Spacious).label("Spacious")),
            )),
    )
}

fn about() -> impl View {
    let theme = use_theme();
    let (checking, confirm, details) = (signal(false), signal(false), signal(false));
    let version = resource(
        move || checking.get(),
        |checking| async move {
            if checking {
                sleep(Duration::from_millis(900)).await;
            }
            checking.then_some("0.1.0 is the latest version")
        },
    );
    let status = move || match (checking.get(), version.loading(), version.get().flatten()) {
        (true, false, Some(message)) => message.to_owned(),
        (true, _, _) => "Checking…".to_owned(),
        _ => "Not checked yet".to_owned(),
    };
    let busy = move || checking.get() && version.loading();
    let info = popover(
        details,
        button("Details").on_click(move || details.update(|open| *open = !*open)),
        move || {
            div().p(12.0).w(220.0).child(
                text("Rouge is a signal-driven, GPU-composited UI framework.")
                    .color(theme.text)
                    .text_size(13.0),
            )
        },
    );
    let reset = move || {
        div()
            .flex()
            .col()
            .gap(12.0)
            .w(280.0)
            .child(
                text("Reset all settings?")
                    .color(theme.text)
                    .text_size(16.0)
                    .bold(),
            )
            .child(
                text("This cannot be undone.")
                    .color(theme.text_muted)
                    .text_size(13.0),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(8.0)
                    .child(button("Cancel").on_click(move || confirm.set(false)))
                    .child(
                        button("Reset")
                            .primary()
                            .on_click(move || confirm.set(false)),
                    ),
            )
    };
    let actions = [
        menu_item("Copy version", || {}),
        menu_item("Report a problem", || {}),
    ];
    div().flex().col().gap(12.0).p(16.0).child(context_menu(
        section("ABOUT")
            .child(row(
                "Version",
                text(status)
                    .color(theme.text_muted)
                    .text_size(theme.text_size),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(8.0)
                    .py(8.0)
                    .child(
                        button("Check for updates")
                            .primary()
                            .enabled(move || !busy())
                            .on_click(move || checking.set(true)),
                    )
                    .child(show(busy, spinner))
                    .child(info)
                    .child(div().flex_1())
                    .child(button("Reset…").on_click(move || confirm.set(true))),
            )
            .child(separator())
            .child(
                text("Right-click this card for more actions.")
                    .color(theme.text_muted)
                    .text_size(12.0)
                    .pt(8.0),
            )
            .child(modal(confirm, reset)),
        actions,
    ))
}

pub(crate) fn settings() -> impl View {
    let theme = use_theme();
    let selected = signal(0usize);
    let dark = signal(false);
    let (name, notes) = (signal("Ada Lovelace".to_owned()), signal(String::new()));
    let saved = signal(0u32);
    let pages = tabs(selected)
        .tab("General", move || {
            scroll_view(scroll().child(general(name, notes, saved))).size_full()
        })
        .tab("Appearance", move || appearance(dark))
        .tab("About", about)
        .size_full();
    div()
        .size_full()
        .bg(theme.background)
        .bind_key(Chord::new(Key::character("s"), Modifiers::COMMAND), Save)
        .on_action(move |_: &Save| saved.update(|count| *count += 1))
        .child(pages)
}

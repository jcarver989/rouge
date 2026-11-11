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

#[component]
fn row(label: &'static str, children: impl View) -> impl View {
    let theme = use_theme();
    view! {
        <div flex items_center justify_between gap=16.0 py=8.0>
            <text color=theme.text text_size=theme.text_size>{label}</text>
            {children}
        </div>
    }
}

#[component]
fn section(title: &'static str) -> Div {
    let theme = use_theme();
    view! {
        <div flex col p=16.0 rounded=12.0 bg=theme.surface border=1.0 border_color=theme.border>
            <text color=theme.text_muted text_size=12.0 bold>{title}</text>
        </div>
    }
}

#[component]
fn general(name: Signal<String>, notes: Signal<String>, saved: Signal<u32>) -> impl View {
    let theme = use_theme();
    let (notify, sounds) = (signal(true), signal(false));
    let volume = signal(60.0f32);
    let language = signal("en");
    let languages = [("en", "English"), ("fr", "Français"), ("ja", "日本語")];
    let options = languages.map(|(code, label)| (code, label.to_owned()));
    view! {
        <div flex col gap=12.0 p=16.0>
            <section title="PROFILE">
                <row label="Display name">
                    <text_input value=name placeholder="Your name" w=220.0/>
                </row>
                <row label="Language">
                    <select value=language options w=220.0/>
                </row>
                <text_area value=notes placeholder="Notes" w_full/>
                <text color=theme.text_muted text_size=12.0>"Saved {saved} times (⌘S)"</text>
            </section>
            <section title="NOTIFICATIONS">
                <row label="Allow notifications">
                    <tooltip label="Show banners and badges"><toggle checked=notify/></tooltip>
                </row>
                <row label="Play sounds">
                    <checkbox checked=sounds label="Enabled"/>
                </row>
                <row label="Volume">
                    <div flex items_center gap=8.0>
                        <icon path=Path::svg(BELL) stroke=1.8 color=theme.text_muted size=18.0/>
                        <slider value=volume range=(0.0, 100.0) step=5.0 w=180.0/>
                    </div>
                </row>
                <progress value={move || volume.get() / 100.0} w_full/>
            </section>
        </div>
    }
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
        view! {
            <div size=22.0 rounded=11.0 bg=swatches[index] border=2.0 cursor=Cursor::Pointer
                 border_color={move || if accent.get() == index { theme.text.get() } else { Color::TRANSPARENT }}
                 on_click={move |_| accent.set(index)}/>
        }
    };
    view! {
        <div flex col gap=12.0 p=16.0>
            <section title="THEME">
                <row label="Dark mode"><toggle checked=dark/></row>
                <row label="Accent">
                    <div flex gap=8.0>{..(0..swatches.len()).map(swatch)}</div>
                </row>
                <row label="Density">
                    <div flex gap=12.0>
                        <radio selected=density value=Density::Compact label="Compact"/>
                        <radio selected=density value=Density::Comfortable label="Comfortable"/>
                        <radio selected=density value=Density::Spacious label="Spacious"/>
                    </div>
                </row>
            </section>
        </div>
    }
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
    let actions = [
        menu_item("Copy version", || {}),
        menu_item("Report a problem", || {}),
    ];
    let toggle_details = move || details.update(|open| *open = !*open);
    view! {
        <div flex col gap=12.0 p=16.0>
            <context_menu items=actions>
                <section title="ABOUT">
                    <row label="Version">
                        <text color=theme.text_muted text_size=theme.text_size>{status}</text>
                    </row>
                    <div flex items_center gap=8.0 py=8.0>
                        <button label="Check for updates" primary
                                enabled={move || !busy()} on_click={move || checking.set(true)}/>
                        <show when=busy><spinner/></show>
                        <popover open=details
                                 trigger={view! { <button label="Details" on_click=toggle_details/> }}>
                            <div p=12.0 w=220.0>
                                <text color=theme.text text_size=13.0>
                                    "Rouge is a signal-driven, GPU-composited UI framework."
                                </text>
                            </div>
                        </popover>
                        <div flex_1/>
                        <button label="Reset…" on_click={move || confirm.set(true)}/>
                    </div>
                    <separator/>
                    <text color=theme.text_muted text_size=12.0 pt=8.0>
                        "Right-click this card for more actions."
                    </text>
                    <modal open=confirm>
                        <div flex col gap=12.0 w=280.0>
                            <text color=theme.text text_size=16.0 bold>"Reset all settings?"</text>
                            <text color=theme.text_muted text_size=13.0>"This cannot be undone."</text>
                            <div flex justify_end gap=8.0>
                                <button label="Cancel" on_click={move || confirm.set(false)}/>
                                <button label="Reset" primary on_click={move || confirm.set(false)}/>
                            </div>
                        </div>
                    </modal>
                </section>
            </context_menu>
        </div>
    }
}

pub(crate) fn settings() -> impl View {
    let theme = use_theme();
    let selected = signal(0usize);
    let dark = signal(false);
    let (name, notes) = (signal("Ada Lovelace".to_owned()), signal(String::new()));
    let saved = signal(0u32);
    let general_tab = move || {
        view! {
            <scroll_view size_full>
                <scroll><general name notes saved/></scroll>
            </scroll_view>
        }
    };
    view! {
        <div size_full bg=theme.background
             bind_key=(Chord::new(Key::character("s"), Modifiers::COMMAND), Save)
             on_action={move |_: &Save| saved.update(|count| *count += 1)}>
            <tabs selected size_full
                  tab=("General", general_tab)
                  tab=("Appearance", move || appearance(dark))
                  tab=("About", about)/>
        </div>
    }
}

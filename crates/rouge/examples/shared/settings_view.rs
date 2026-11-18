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
    view! {
        <div flex items_center justify_between gap=16.0 py=8.0>
            <text color=Role::Text>{label}</text>
            {children}
        </div>
    }
}

#[component]
fn section(title: &'static str) -> Div {
    view! {
        <div flex col p=16.0 rounded={Corners::all(12.0)} bg=Role::Elevated border=1.0 border_color=Role::Border>
            <text color=Role::TextMuted text_size=12.0 bold>{title}</text>
        </div>
    }
}

#[component]
fn general(name: Signal<String>, notes: Signal<String>, saved: Signal<u32>) -> impl View {
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
                <text_input multiline value=notes placeholder="Notes" w_full/>
                <text color=Role::TextMuted text_size=12.0>"Saved {saved} times (⌘S)"</text>
            </section>
            <section title="NOTIFICATIONS">
                <row label="Allow notifications">
                    <tooltip label="Show banners and badges"><toggle checked=notify/></tooltip>
                </row>
                <row label="Play sounds">
                    <div flex items_center gap=8.0>
                        <checkbox test_id="sounds" checked=sounds/>
                        <text color=Role::Text>"Enabled"</text>
                    </div>
                </row>
                <row label="Volume">
                    <div flex items_center gap=8.0>
                        <icon path=Path::svg(BELL) stroke=1.8 color=Role::TextMuted size=18.0/>
                        <slider value=volume range={(0.0, 100.0)} step=5.0 w=180.0/>
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
    let theme = Theme::current();
    effect(move || {
        theme.set(UiPalette {
            accent: Rgba::rgb(swatches[accent.get()]),
            ..UiPalette::for_dark(dark.get())
        });
    });
    let gap = move || match density.get() {
        Density::Compact => 4.0,
        Density::Comfortable => 8.0,
        Density::Spacious => 12.0,
    };
    let swatch = move |index: usize| {
        view! {
            <div size=22.0 rounded={Corners::all(11.0)} bg=swatches[index] border=2.0 cursor=Cursor::Pointer
                 border_color={move || if accent.get() == index { theme.get().text } else { Color::TRANSPARENT }}
                 on_click={move |_| accent.set(index)}/>
        }
    };
    view! {
        <div flex col gap=12.0 p=16.0>
            <section title="THEME">
                <row label="Dark mode"><toggle checked=dark/></row>
                <row label="Accent">
                    <div flex gap=gap>{..(0..swatches.len()).map(swatch)}</div>
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
    let (confirm, details) = (signal(false), signal(false));
    let check = action(|()| async {
        sleep(Duration::from_millis(900)).await;
        "0.1.0 is the latest version"
    });
    let status = move || match (check.pending(), check.value()) {
        (true, _) => "Checking…",
        (false, Some(message)) => message,
        (false, None) => "Not checked yet",
    };
    let busy = move || check.pending();
    let actions = [
        MenuItem::callback("Copy version", || {}),
        MenuItem::callback("Report a problem", || {}),
    ];
    view! {
        <div flex col gap=12.0 p=16.0>
            <context_menu items=actions>
                <section title="ABOUT">
                    <row label="Version">
                        <text color=Role::TextMuted>{status}</text>
                    </row>
                    <div flex items_center gap=8.0 py=8.0>
                        <button test_id="check" label="Check for updates" variant=ButtonVariant::Primary
                                        disabled=busy on_click={move || check.dispatch(())}/>
                        <show when=busy><spinner/></show>
                        <popover open=details
                                 trigger={view! { <button test_id="details" label="Details"/> }}>
                            <div p=12.0 w=220.0>
                                <text color=Role::Text text_size=13.0>
                                    "Rouge is a signal-driven, GPU-composited UI framework."
                                </text>
                            </div>
                        </popover>
                        <div flex_1/>
                        <button test_id="reset" label="Reset…" on_click={move || confirm.set(true)}/>
                    </div>
                    <separator/>
                    <text color=Role::TextMuted text_size=12.0 pt=8.0>
                        "Right-click this card for more actions."
                    </text>
                    <show when={move || confirm.get()}>
                        <modal test_id="confirm" title="Reset all settings?" hint="This cannot be undone."
                               modal_size=ModalSize::Compact on_dismiss={move || confirm.set(false)}>
                            <div flex justify_end gap=8.0>
                                <button test_id="cancel" label="Cancel" on_click={move || confirm.set(false)}/>
                                <button test_id="confirm-reset" label="Reset" variant=ButtonVariant::Primary
                                        on_click={move || confirm.set(false)}/>
                            </div>
                        </modal>
                    </show>
                </section>
            </context_menu>
        </div>
    }
}

pub(crate) fn settings() -> impl View {
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
        <div size_full bg=Role::Canvas
             bind_key=(Chord::new(Key::character("s"), Modifiers::COMMAND), Save)
             on_action={move |_: &Save| saved.update(|count| *count += 1)}>
            <tabs selected size_full
                  tab=("General", general_tab)
                  tab=("Appearance", move || appearance(dark))
                  tab=("About", about)/>
        </div>
    }
}

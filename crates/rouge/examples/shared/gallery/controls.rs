use super::{COLUMN, Cell, band, cell, choice, column, row, say, section};
use rouge::core::{AnyView, Div, Styled, View, div, text, view};
use rouge::reactive::{Signal, keyed_list, signal, sleep, spawn};
use rouge::widgets::tokens::{Hold, ROW_HEIGHT, Space};
use rouge::widgets::{
    ButtonVariant, CheckState, ControlSize, IconName, NoticeTone, UiStyles, badge, button,
    checkbox, code_input, field, icon_button, number_field, radio, segmented_control, select,
    slider, tag_input, text_input, toggle,
};
use std::time::Duration;

pub(crate) fn controls(status: Signal<String>) -> Div {
    section(
        "Controls",
        "Flat fills with hairline edges. A press scales with Pace::Press; focus is an accent border.",
        [
            band([buttons(status), sizes(status)]),
            band([toggles(), checkboxes(), radios(), segments()]),
            band([sliders(), inputs(), selects(), icon_buttons(status)]),
            band([fields(), numbers(), tags(), codes(status)]),
        ],
    )
}

fn buttons(status: Signal<String>) -> Cell {
    let slot = |content: AnyView| view! { <div flex items_center w=COLUMN h=ROW_HEIGHT flex_none>{content}</div> };
    let heading = ["", "Rest", "Disabled", "Loading"]
        .map(|name| slot(view! { <text small muted>{name}</text> }.into_any()));
    let variants = [
        ("Primary", "Save", ButtonVariant::Primary),
        ("Secondary", "Cancel", ButtonVariant::Secondary),
        ("Ghost", "Skip", ButtonVariant::Ghost),
        ("Destructive", "Delete", ButtonVariant::Destructive),
    ]
    .map(|(name, label, variant)| {
        let id = name.to_lowercase();
        let rest = view! {
            <button label=label.to_owned() test_id=id.clone() variant=variant ripple
                 on_click=say(status, label)/>
        };
        let disabled = view! {
            <button label=label.to_owned() test_id={format!("{id}-disabled")} variant=variant disabled=true/>
        };
        let loading = view! {
            <button label=label.to_owned() test_id={format!("{id}-loading")} variant=variant loading=true/>
        };
        view! {
            <div flex>
                {slot(view! { <text small muted>{name}</text> }.into_any())}
                {slot(rest.into_any())}
                {slot(disabled.into_any())}
                {slot(loading.into_any())}
            </div>
        }
    });
    let content = view! {
        <column gap=Space::X3>
            <div flex>{..heading}</div>
            {..variants}
        </column>
    };
    cell(
        "button",
        2.0,
        "Rest is live: hover it, press it for the ripple at the pointer, or Tab to it for the focus line. Loading keeps the width of its label.",
        content,
    )
    .spec("28 px, radius 8, 12 px padding")
}

fn sizes(status: Signal<String>) -> Cell {
    let publishing = signal(false);
    let publish = move || {
        publishing.set(true);
        spawn(async move {
            sleep(Duration::from_millis(1500)).await;
            publishing.set(false);
            status.set("Published".into());
        });
    };
    let sized = |id: &'static str, label: &'static str, size: ControlSize| {
        view! {
            <button label=label.to_owned() test_id=id variant=ButtonVariant::Primary control_size=size/>
        }
            .into_any()
    };
    let rows = [
        ("Small", sized("small", "Add", ControlSize::Small)),
        (
            "Medium",
            sized("medium", "Add to queue", ControlSize::Medium),
        ),
        ("Large", sized("large", "Add to queue", ControlSize::Large)),
        (
            "Loading",
            view! {
                <button label="Publish".to_owned() test_id="publish" variant=ButtonVariant::Primary
                     loading=publishing on_click=publish/>
            }
                .into_any(),
        ),
        (
            "Hold",
            view! {
                <button label="Hold to delete".to_owned() test_id="hold" variant=ButtonVariant::Destructive
                     confirm=Hold::seconds(1.0) on_click=say(status, "Library deleted")/>
            }
                .into_any(),
        ),
    ]
    .map(|(name, control)| {
        view! {
            <div flex items_center>
                <div w=COLUMN flex_none><text small muted>{name}</text></div>
                {control}
            </div>
        }
    });
    cell(
        "ControlSize · confirm",
        1.0,
        "Large is the touch size. A destructive action fills while held and springs back if released early, in place of a confirmation dialog.",
        view! { <column gap=Space::X3>{..rows}</column> },
    )
    .spec("24, 28, 44")
}

fn icon_buttons(status: Signal<String>) -> Cell {
    let content = view! {
        <row>
            <icon_button icon=IconName::Pin label="Love".to_owned() shortcut="l"
                 on_click=say(status, "Loved")/>
            <icon_button icon=IconName::Copy label="Copy link".to_owned() test_id="copy"
                 shortcut="secondary-c" on_click=say(status, "Copied")/>
            <icon_button icon=IconName::Ellipsis label="More".to_owned() test_id="more"/>
            <badge tone=NoticeTone::Positive>{"Synced".to_owned()}</badge>
        </row>
    };
    cell(
        "icon_button",
        1.0,
        "Hover for a glass tooltip with its shortcut in a kbd. The badge shows the positive role beside the accent.",
        content,
    )
}

fn labelled(label: &'static str, control: AnyView) -> Div {
    view! {
        <div flex items_center justify_between gap=12.0>
            <text body primary>{label}</text>
            {control}
        </div>
    }
}

fn toggles() -> Cell {
    let (crossfade, normalise) = (signal(true), signal(false));
    let content = view! {
        <column gap=12.0>
            {labelled(
                "Crossfade",
                view! { <toggle checked=crossfade test_id="crossfade"/> }.into_any(),
            )}
            {labelled("Normalise volume", view! { <toggle checked=normalise/> }.into_any())}
        </column>
    };
    cell(
        "toggle",
        1.0,
        "The thumb stretches while pressed, then springs across.",
        content,
    )
}

fn checkboxes() -> Cell {
    let (albums, playlists) = (signal(true), signal(false));
    let all = signal(CheckState::Indeterminate);
    let option = |control: AnyView, label: &'static str| {
        view! {
            <div flex items_center gap=8.0>
                {control}
                <text body primary>{label}</text>
            </div>
        }
    };
    let content = view! {
        <column gap=12.0>
            {option(view! { <checkbox checked=all/> }.into_any(), "Download for offline")}
            <div pl=24.0>{option(view! { <checkbox checked=albums/> }.into_any(), "Albums")}</div>
            <div pl=24.0>{option(view! { <checkbox checked=playlists/> }.into_any(), "Playlists")}</div>
        </column>
    };
    cell("checkbox", 1.0, "", content)
}

fn radios() -> Cell {
    let quality = signal("lossless");
    let content = view! {
        <column gap=12.0>
            {..[
                view! { <radio selected=quality value="lossless" label="Lossless"/> },
                view! { <radio selected=quality value="high" label="High"/> },
                view! { <radio selected=quality value="saver" label="Data saver"/> },
            ]}
        </column>
    };
    cell("radio", 1.0, "", content)
}

fn segments() -> Cell {
    let (layout, range) = (signal(0usize), signal(1usize));
    let pick = |group: Signal<usize>, name: &str, index: usize, label| {
        choice(group, index, label, format!("{name}-{index}"))
    };
    let content = view! {
        <column gap=12.0 items_start>
            <segmented_control test_id="layout">
                {..[
                    pick(layout, "layout", 0, "Grid"),
                    pick(layout, "layout", 1, "List"),
                ]}
            </segmented_control>
            <segmented_control test_id="range">
                {..[
                    pick(range, "range", 0, "Day"),
                    pick(range, "range", 1, "Week"),
                    pick(range, "range", 2, "Month"),
                ]}
            </segmented_control>
        </column>
    };
    cell(
        "segmented_control",
        1.0,
        "One indicator layer, moved with Pace::Move.",
        content,
    )
}

fn sliders() -> Cell {
    let (volume, balance) = (signal(64.0), signal(50.0));
    let content = view! {
        <column gap=16.0>
            <slider value=volume test_id="volume" range={(0.0, 100.0)} step=1.0
                 format={|value| format!("{value:.0}%")} w_full/>
            <slider value=balance range={(0.0, 100.0)} step=1.0
                 format={|value| format!("{:+.1} dB", (value - 50.0) / 5.0)} w_full/>
        </column>
    };
    cell(
        "slider",
        1.0,
        "A glass value tip on hover and drag; the thumb grows while held.",
        content,
    )
}

fn inputs() -> Cell {
    let (name, link) = (signal(String::from("Late night")), signal(String::new()));
    let field = |label: &'static str, input: AnyView| {
        view! {
            <div flex col gap=4.0>
                <text label primary>{label}</text>
                {input}
            </div>
        }
    };
    let content = view! {
        <column gap=12.0>
            <field label="Playlist name">{view! { <text_input value=name w_full/> }.into_any()}</field>
            <field label="Share link">
                {view! { <text_input value=link placeholder="rouge.fm/late-night" w_full/> }
                    .into_any()}
            </field>
        </column>
    };
    cell("text_input", 1.0, "", content)
}

fn selects() -> Cell {
    let (sort, quality) = (signal(0usize), signal(0usize));
    let sorts = [(0, "Recently added"), (1, "Title"), (2, "Artist")]
        .map(|(value, label)| (value, label.to_owned()));
    let qualities = [(0, "Lossless"), (1, "High"), (2, "Data saver")]
        .map(|(value, label)| (value, label.to_owned()));
    let content = view! {
        <column gap=12.0>
            <select value=sort options=sorts/>
            <select value=quality options=qualities/>
        </column>
    };
    cell(
        "select",
        1.0,
        "Shares the menu renderer with context_menu.",
        content,
    )
}

fn fields() -> Cell {
    let name = signal(String::new());
    let error = move || {
        name.with(String::is_empty)
            .then(|| "A playlist needs a name".to_owned())
    };
    let link = signal(String::from("late-night"));
    let content = view! {
        <column gap=12.0>
            <field label="Playlist name".to_owned() test_id="name-field" hint="Shown on your profile"
                 error=error>
                <text_input value=name w_full/>
            </field>
            <field label="Share link".to_owned() hint="rouge.fm/ followed by this">
                <text_input value=link w_full/>
            </field>
        </column>
    };
    cell(
        "field",
        1.0,
        "A label, a hint and an error. The error replaces the hint and turns the border destructive.",
        content,
    )
}

fn numbers() -> Cell {
    let (gain, tempo) = (signal(2.5), signal(96.0));
    let content = view! {
        <column gap=12.0 items_start>
            <number_field value=gain test_id="gain" label="Gain" range={(-12.0, 12.0)} step=0.5 suffix="dB"/>
            <number_field value=tempo label="Tempo" range={(40.0, 220.0)} suffix="bpm"/>
        </column>
    };
    cell(
        "number_field",
        1.0,
        "Drag the label to scrub, type to edit, arrows to step.",
        content,
    )
}

fn tags() -> Cell {
    let tags = keyed_list(String::clone);
    tags.extend(["ambient", "late night", "vinyl"].map(str::to_owned));
    let content = view! { <tag_input tags=tags test_id="tags" placeholder="Add a tag" w_full/> };
    cell(
        "tag_input",
        1.0,
        "Enter adds, Backspace on an empty input removes the last. Each tag enters and exits on its own.",
        content,
    )
}

fn codes(status: Signal<String>) -> Cell {
    let code = signal(String::from("48"));
    let content = view! {
        <code_input value=code length=6 test_id="code"
             on_complete={move |code: String| status.set(format!("Entered {code}"))}/>
    };
    cell(
        "code_input",
        1.0,
        "Cells drawn over one editable, so paste and IME behave like a text input.",
        view! { <row>{content}</row> },
    )
}

use super::stage::ALBUMS;
use super::{Cell, band, cell, column, section};
use rouge::core::{Div, Role, Styled, View, div, presentation, scroll, text, view};
use rouge::reactive::{Signal, effect, signal, sleep, spawn};
use rouge::widgets::tokens::Radius;
use rouge::widgets::{
    ButtonVariant, Detent, IconName, UiStyles, button, list_row, muted_text, row_action,
    scroll_view, sheet, tabs,
};
use std::time::Duration;

const ADAPTS: [(&str, &str); 6] = [
    (
        "popover · select · context_menu",
        "An anchored glass panel with a pointer. A bottom sheet on a handheld.",
    ),
    (
        "modal",
        "A centred dialog. A full-height sheet on a handheld.",
    ),
    (
        "tabs",
        "An underlined strip. With .bar(), a floating glass bar on a handheld.",
    ),
    (
        "list_row",
        "Actions on hover with a pointer. Swipe actions on touch.",
    ),
    (
        "button",
        "28 px with a pointer, 44 px on touch, unless sized explicitly.",
    ),
    ("scroll_view", "Pull past the top to refresh."),
];

pub(crate) fn touch(status: Signal<String>) -> Div {
    section(
        "Touch",
        "One set for pointer and touch. Components read one Presentation from context, so the same code is a popover on a desktop and a sheet on a phone.",
        [band([device(status), adaptive()])],
    )
}

pub(crate) fn library(status: Signal<String>) -> impl View {
    let (tab, queue, refreshing) = (signal(0usize), signal(false), signal(false));
    effect(move || {
        if refreshing.get() {
            spawn(async move {
                sleep(Duration::from_millis(1200)).await;
                refreshing.set(false);
                status.set("Library refreshed".into());
            });
        }
    });
    let albums = move || {
        let rows = ALBUMS[5..].iter().map(|album| album.title).map(|title| {
            view! {
                <list_row test_id={format!("track-{title}")}
                     on_click={move || status.set(format!("Playing {title}"))}
                     action={row_action("Queue", move || status.set(format!("Queued {title}")))
                         .icon(IconName::Plus)}
                     action={row_action("Remove", move || status.set(format!("Removed {title}")))
                         .destructive()}>
                    {art()}
                    <text label primary no_wrap>{title}</text>
                </list_row>
            }
        });
        let list = view! {
            <div flex col p=8.0 pb=96.0>
                <div flex items_center justify_between p=8.0>
                    <text heading primary>"Library"</text>
                    <button label="Up next" test_id="open-sheet" variant=ButtonVariant::Secondary
                         on_click={move || queue.set(true)}/>
                </div>
                {..rows}
            </div>
        };
        view! {
            <scroll_view refresh=refreshing size_full>
                <scroll>{list}</scroll>
            </scroll_view>
        }
    };
    let placeholder = |label: &'static str| move || view! { <div size_full flex items_center justify_center><muted_text>{label}</muted_text></div> };
    let up_next = move || {
        let rows = ALBUMS[5..].iter().map(|album| album.title).map(|title| {
            view! {
                <list_row>
                    {art()}
                    <text label primary no_wrap>{title}</text>
                </list_row>
            }
        });
        view! {
            <div flex col flex_1 min_h=0.0 px=8.0>
                <div px=8.0 pb=8.0><text heading primary>"Up next"</text></div>
                <scroll flex_1 min_h=0.0>
                    <div flex col>{..rows}</div>
                </scroll>
            </div>
        }
    };
    view! {
        <div size_full>
            <tabs selected=tab test_id="touch-tabs" bar icon_tab=(IconName::Folder, "Library", albums)
                 icon_tab=(IconName::Search, "Search", placeholder("Search your library"))
                 icon_tab=(IconName::Settings, "Settings", placeholder("Nothing to set")) size_full/>
            <sheet open=queue children=up_next detents=[Detent::Medium, Detent::Large] test_id="up-next"/>
        </div>
    }
}

fn device(status: Signal<String>) -> Cell {
    let presentation = presentation();
    let reading = move || {
        let now = presentation.get();
        format!("Presentation here: {:?}, {:?}", now.input, now.width)
    };
    let frame = view! {
        <div h=420.0 rounded=Radius::Floating border=1.0 border_color=Role::BorderSubtle
             bg=Role::Surface overflow_hidden>
            {library(status)}
        </div>
    };
    cell(
        "tabs · list_row · scroll_view · sheet",
        1.0,
        "The same view the phone snapshot shows. Hover a row for its actions; Up next opens a sheet with two detents.",
        view! {
            <column>
                {frame}
                <text small muted>{reading}</text>
            </column>
        },
    )
}

fn adaptive() -> Cell {
    let rows = ADAPTS.map(|(name, behaviour)| {
        view! {
            <div flex col gap=4.0 py=8.0 border_b=1.0 border_color=Role::BorderSubtle>
                <text code text_size=12.0 secondary>{name}</text>
                <text small primary>{behaviour}</text>
            </div>
        }
    });
    cell(
        "Adaptive presentation",
        1.0,
        "Touch and compact width together make a handheld. The host reports the input kind; the width class follows the window.",
        view! { <div flex col>{..rows}</div> },
    )
}

fn art() -> Div {
    view! { <div flex_none size=24.0 rounded=Radius::Inner bg=Role::Accent/> }
}

use super::stage::ALBUMS;
use super::{Cell, band, cell, column, section};
use rouge::core::{
    Div, Role, ScrollState, Scrollable, Styled, View, div, presentation, scroll, text, view,
    window_handle,
};
use rouge::reactive::{Signal, effect, signal, sleep, spawn};
use rouge::widgets::tokens::{Radius, Space};
use rouge::widgets::{
    ButtonVariant, Date, Detent, IconName, Material, MaterialExt, UiStyles, action_bar, alert,
    button, chip, chip_row, date_picker, fab, icon_button, icon_tile, list_group, list_row,
    muted_text, nav_bar, nav_stack, number_field, page_dots, pager, row_action, scroll_view,
    search_field, sheet, tabs, toggle,
};
use std::time::Duration;

const ADAPTS: [(&str, &str); 12] = [
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
    (
        "toggle · checkbox · radio · slider · text_input",
        "Pointer sized by default. Larger on touch, inside a 44 px target.",
    ),
    (
        "number_field",
        "Scrub the label and use arrows with a pointer. Minus and plus buttons on touch.",
    ),
    (
        "action_bar",
        "A strip of buttons. On a handheld, a floating bar in the tab bar's slot.",
    ),
    (
        "resizable_panel",
        "A sidebar beside the page. With .open(), a drawer over it on a handheld.",
    ),
    (
        "toast",
        "A stack in the corner. On a handheld, one at a time across the window, swiped away.",
    ),
    (
        "context_menu",
        "A right click with a pointer. A long press on touch.",
    ),
];

pub(crate) fn touch(status: Signal<String>) -> Div {
    section(
        "Touch",
        "One set for pointer and touch. Components read one Presentation from context, so the same code is a popover on a desktop and a sheet on a phone.",
        [
            band([device(status), adaptive()]),
            band([screens(status), pickers(status)]),
        ],
    )
}

fn screens(status: Signal<String>) -> Cell {
    let frame = view! {
        <div relative h=420.0 rounded=Radius::Floating border=1.0 border_color=Role::BorderSubtle
             bg=Role::Canvas overflow_hidden>
            {inbox(status)}
        </div>
    };
    cell(
        "nav_stack · nav_bar · search_field · chip · list_group · fab · alert",
        1.0,
        "Two screens on the window's route. Scroll to fold the large title; a row or the gear pushes Settings, and its back button, Escape or a swipe from the leading edge pops it.",
        frame,
    )
}

pub(crate) fn inbox(status: Signal<String>) -> impl View {
    let window = window_handle();
    let (query, unread, flagged) = (signal(String::new()), signal(false), signal(false));
    let (alerts, storage, confirm) = (signal(true), signal(5.0), signal(false));
    let root = move |_| {
        let state = signal(ScrollState::default());
        let rows = ALBUMS[..6].iter().map(|album| album.title).map(|title| {
            view! {
                <list_row chevron detail="Updated today" leading={art()}
                     on_click={move || window.navigate("/settings")}>
                    <text label primary no_wrap>{title}</text>
                </list_row>
            }
        });
        view! {
            <div relative flex col size_full>
                <nav_bar title="Inbox" test_id="inbox-bar" large scroll=state
                     trailing={icon_button(IconName::Settings, "Settings".to_owned())
                         .test_id("open-settings")
                         .on_click(move || window.navigate("/settings"))}/>
                <scroll flex_1 min_h=0.0 scroll_state=state>
                    <div flex col gap=Space::X3 p=Space::X3 pb=Space::X24>
                        <search_field value=query placeholder="Search"/>
                        <chip_row>
                            <chip label="Unread".to_owned() count=12usize selected=unread
                                 on_click={move || unread.toggle()}/>
                            <chip label="Flagged".to_owned() selected=flagged
                                 on_click={move || flagged.toggle()}/>
                            <chip label="Attachments".to_owned()/>
                        </chip_row>
                        <list_group>{..rows}</list_group>
                    </div>
                </scroll>
                <fab icon=IconName::Pencil label="Compose" test_id="compose"
                     on_click={move || status.set("Compose".into())}/>
            </div>
        }
    };
    let settings = move |_| {
        view! {
            <div flex col size_full>
                <nav_bar title="Settings" test_id="settings-bar"/>
                <div flex col gap=Space::X4 p=Space::X3>
                    <list_group title="Device">
                        <list_row leading={icon_tile(IconName::Bell)} trailing={toggle(alerts)}>
                            <text label primary>"Notifications"</text>
                        </list_row>
                        <list_row leading={icon_tile(IconName::Cloud)}
                             trailing={number_field(storage).range((1.0, 50.0)).suffix("GB")}>
                            <text label primary>"Offline storage"</text>
                        </list_row>
                    </list_group>
                    <button label="Delete account".to_owned() variant=ButtonVariant::Destructive
                         on_click={move || confirm.set(true)}/>
                </div>
                <alert open=confirm title="Delete this account?" message="This can't be undone."
                     action={row_action("Delete", move || status.set("Account deleted".into())).destructive()}
                     cancel="Cancel"/>
            </div>
        }
    };
    view! { <nav_stack page=("/", root) page=("/settings", settings) size_full/> }
}

fn pickers(status: Signal<String>) -> Cell {
    let (page, due) = (signal(0usize), signal(Date::new(2026, 11, 20)));
    let cards = ["3 reviews waiting", "Release on Friday", "12 unread"].map(|title| {
        view! {
            <div flex col justify_end h=120.0 p=Space::X4 material=Material::Surface>
                <text heading primary>{title}</text>
            </div>
        }
    });
    let chosen = move || {
        let date = due.get();
        format!("Due {}-{:02}-{:02}", date.year, date.month, date.day)
    };
    cell(
        "pager · page_dots · date_picker · wheel · action_bar",
        1.0,
        "Drag the cards or the wheels; both settle on a whole step. The bar's icon actions are buttons here and a floating bar on a handheld.",
        view! {
            <column>
                {pager(page).children(cards)}
                {page_dots(page, 3)}
                <date_picker value=due years={2020..=2032}/>
                <text small muted>{chosen}</text>
                <action_bar action={row_action("Archive", move || status.set("Archived".into())).icon(IconName::Archive)}
                     action={row_action("Delete", move || status.set("Deleted".into())).icon(IconName::X).destructive()}>
                    <button label="Save".to_owned() variant=ButtonVariant::Primary/>
                </action_bar>
            </column>
        },
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

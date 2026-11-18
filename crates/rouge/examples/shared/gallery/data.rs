use super::stage::ALBUMS;
use super::{Cell, band, cell, column, row, section};
use rouge::core::{Div, Role, Styled, View, div, text, view};
use rouge::reactive::{Signal, effect, keyed_list, signal};
use rouge::widgets::{
    ButtonVariant, ControlSize, Icon, IconName, InterfaceSize, MenuItem, NoticeTone, PickerItem,
    SelectionState, Sort, UiStyles, action_bar, avatar, avatar_group, badge, breadcrumb, button,
    column as table_column, context_menu, data_table, disclosure, empty_state, inline_edit,
    muted_text, notification, picker_list, progress, section_header, skeleton, sortable, spinner,
    tabs, tree_row,
};

pub(crate) fn data(size: InterfaceSize, status: Signal<String>) -> Div {
    section(
        "Data and structure",
        "Trees, tabs, disclosure, notices and the status bar.",
        [
            band([table(status), queue()]),
            band([tree(status), structure(), notices(status)]),
            band([empty(status), picker(status), feedback()]),
            band([people(), paths(status), editing(size, status)]),
        ],
    )
}

fn tree(status: Signal<String>) -> Cell {
    let count = view! { <text caption muted>"3"</text> };
    let pick = move |name: &'static str| move || status.set(format!("Selected {name}"));
    let content = view! {
        <div flex col>
            <section_header test_id="files" title="Ines Okafor" expanded=true badge=count.into_any()/>
            <tree_row test_id="album" label="Soft Machinery" expanded=true icon=IconName::FolderOpen/>
            <tree_row test_id="first" label="Glasswork" depth=1 icon=IconName::AudioLines
                      on_click={pick("Glasswork")}/>
            <tree_row test_id="second" label="Harmattan" depth=1 icon=IconName::AudioLines
                      selection=SelectionState::Selected on_click={pick("Harmattan")}/>
            <tree_row test_id="closed" label="Low Tide" expanded=false icon=IconName::Folder/>
        </div>
    };
    cell("section_header · tree_row", 1.0, "", content)
}

fn structure() -> Cell {
    let (tab, open) = (signal(0usize), signal(Some(0usize)));
    let fold = move |key: usize, title: &'static str, body: &'static str| {
        view! {
            <disclosure expanded=false accordion=(open, key) test_id={format!("fold-{key}")}
                 body={move || view! { <div pb=8.0><muted_text>{body}</muted_text></div> }}>
                <text body primary>{title}</text>
            </disclosure>
        }
    };
    let pane =
        |body: &'static str| move || view! { <div pt=8.0><muted_text>{body}</muted_text></div> };
    let content = view! {
        <column gap=12.0>
            <tabs selected=tab tab=("Tracks", pane("11 tracks, 43 minutes."))
                 tab=("Credits", pane("Produced by Ines Okafor."))
                 tab=("Notes", pane("Vinyl pressing ships in March."))/>
            <column gap=4.0>
                {fold(
                    0,
                    "Playback",
                    "Crossfade, gapless and volume levelling.",
                )}
                {fold(1, "Downloads", "Lossless over Wi-Fi only.")}
                {fold(2, "Devices", "Kitchen speaker and two more.")}
            </column>
        </column>
    };
    cell(
        "tabs · disclosure",
        1.0,
        "One underline slides with Pace::Move and panels cross-fade. The accordion keeps one section open.",
        content,
    )
}

fn notices(status: Signal<String>) -> Cell {
    let notices = [
        (NoticeTone::Info, "Gapless playback is on"),
        (NoticeTone::Positive, "Library synced"),
        (NoticeTone::Warning, "3 tracks could not sync"),
    ]
    .map(|(tone, message)| view! { <notification message=message.to_owned() tone=tone/> });
    let failed = view! {
        <notification message="Kitchen speaker is offline.".to_owned() tone=NoticeTone::Error
             title?=Some("Could not start playback".to_owned())>
            <button label="Retry" on_click={move || status.set("Retrying".into())}/>
        </notification>
    };
    cell(
        "notification",
        1.0,
        "An optional title and an action slot.",
        view! {
            <column>
                {..notices}
                {failed}
            </column>
        },
    )
}

fn empty(status: Signal<String>) -> Cell {
    let browse = view! {
        <button label="Browse library" test_id="browse" variant=ButtonVariant::Secondary
             control_size=ControlSize::Small on_click={move || status.set("Browsing".into())}/>
    };
    let content = view! {
        <empty_state icon=IconName::AudioLines title="No albums in Late night yet"
                     detail="Add them from your library.">
            {browse}
        </empty_state>
    };
    cell("empty_state", 1.0, "", content)
}

fn picker(status: Signal<String>) -> Cell {
    let highlighted = signal(Some(1usize));
    let albums = &ALBUMS[..4];
    let items = albums
        .iter()
        .map(|album| {
            PickerItem::new(album.title, album.title)
                .icon(IconName::AudioLines)
                .detail(album.artist)
        })
        .collect();
    let content = view! {
        <picker_list items=items test_id="albums" highlighted={move || highlighted.get()}
             on_hover={move |index: usize| highlighted.set(Some(index))}
             on_choose={move |index: usize| status.set(format!("Chose {}", albums[index].title))}/>
    };
    cell(
        "picker_list",
        1.0,
        "One Pigment highlight slides between rows with Pace::Move.",
        content,
    )
}

fn feedback() -> Cell {
    let done = signal(0.64);
    let content = view! {
        <column gap=16.0>
            <row>
                <spinner/>
                <muted_text>"Syncing 214 albums"</muted_text>
            </row>
            <progress value={move || done.get()} w_full/>
        </column>
    };
    cell("spinner · progress", 1.0, "", content)
}

fn editing(size: InterfaceSize, status: Signal<String>) -> Cell {
    let edit = signal(String::from("Late night"));
    let items = [
        MenuItem::callback("Larger", move || size.set(size.get() + 1.0)).icon(IconName::Plus),
        MenuItem::callback("Smaller", move || size.set(size.get() - 1.0)).icon(IconName::Minus),
        MenuItem::Separator,
        MenuItem::callback("Reset", move || size.set(16.0)),
    ];
    let content = view! {
        <column gap=12.0>
            <inline_edit value=edit on_commit={move |value: String| {
                edit.set(value.clone());
                status.set(format!("Renamed to {value}"));
            }}/>
            <action_bar>
                <div flex_1><text caption primary>{move || status.get()}</text></div>
                <context_menu items dropdown>
                    <button test_id="interface-size" label="Interface size"/>
                </context_menu>
            </action_bar>
            <div/>
        </column>
    };
    cell(
        "inline_edit · action_bar",
        1.0,
        "The bar shows the last action taken anywhere on this page.",
        content,
    )
}

fn people() -> Cell {
    let listeners = [
        "Ines Okafor",
        "Marek Havel",
        "Yuki Tan",
        "Sal Choir",
        "Kofi Lund",
    ]
    .map(|name| view! { <avatar name=name.to_owned()/> });
    let badges = view! {
        <row>
            {..[
                view! { <badge tone=NoticeTone::Info>{"Lossless".to_owned()}</badge> },
                view! { <badge tone=NoticeTone::Positive>{"Synced".to_owned()}</badge> },
                view! { <badge tone=NoticeTone::Warning>{"3 pending".to_owned()}</badge> },
                view! { <badge tone=NoticeTone::Error>{"Offline".to_owned()}</badge> },
                view! { <badge tone=NoticeTone::Neutral>{"Draft".to_owned()}</badge> },
            ]}
        </row>
    };
    let loading = view! {
        <div flex items_center gap=12.0>
            <skeleton size=40.0/>
            <div flex col flex_1 gap=8.0>
                <skeleton w=160.0/>
                <skeleton w=96.0 h=12.0/>
            </div>
        </div>
    };
    let content = view! {
        <column gap=16.0>
            {badges}
            <row gap=12.0>
                <avatar name="Ines Okafor".to_owned() avatar_size=40.0/>
                <avatar_group avatars=listeners max=4 test_id="listeners"/>
            </row>
            {loading}
        </column>
    };
    cell(
        "badge · avatar · skeleton",
        2.0,
        "Tones tint a role. The skeleton shimmers with one ShaderEffect on a clock uniform.",
        content,
    )
}

fn paths(status: Signal<String>) -> Cell {
    let go = move |place: &'static str| move || status.set(format!("Opened {place}"));
    let content = view! {
        <breadcrumb test_id="path" link=("Library", go("Library")) link=("Playlists", go("Playlists"))
             current="Late night"/>
    };
    cell("breadcrumb", 1.0, "", content)
}

#[derive(Clone)]
struct Track {
    title: &'static str,
    artist: &'static str,
    plays: u32,
    length: &'static str,
}

const PLAYS: [(u32, &str); 8] = [
    (4120, "3:42"),
    (960, "4:05"),
    (12_800, "2:58"),
    (730, "5:11"),
    (2210, "3:20"),
    (1875, "4:44"),
    (310, "6:02"),
    (5400, "3:33"),
];

fn library() -> Vec<Track> {
    let tracks = ALBUMS
        .iter()
        .zip(PLAYS)
        .map(|(album, (plays, length))| Track {
            title: album.title,
            artist: album.artist,
            plays,
            length,
        });
    tracks.collect()
}

fn table(status: Signal<String>) -> Cell {
    let tracks = keyed_list(|track: &Track| track.title);
    tracks.extend(library());
    let (selection, sort) = (signal(Some("Low Tide")), signal(None::<Sort>));
    effect(move || {
        let Some(sort) = sort.get() else { return };
        let mut ordered = library();
        ordered.sort_by(|a, b| match sort.column {
            0 => a.title.cmp(b.title),
            1 => a.artist.cmp(b.artist),
            _ => a.plays.cmp(&b.plays),
        });
        if sort.descending {
            ordered.reverse();
        }
        for (index, track) in ordered.iter().enumerate() {
            tracks.move_to(&track.title, index);
        }
    });
    let content = view! {
        <data_table rows=tracks test_id="tracks" selection=selection sort=sort
             on_open={move |title: &'static str| status.set(format!("Playing {title}"))}
             column={table_column("Title", |track: &Track| view! { <text body no_wrap>{track.title}</text> })
                 .flex(1.4)
                 .sortable()}
             column={table_column("Artist", |track: &Track| {
                 view! { <text body no_wrap>{track.artist}</text> }
             })
             .sortable()}
             column={table_column("Plays", |track: &Track| {
                 view! { <text body>{track.plays.to_string()}</text> }
             })
             .sortable()}
             column=table_column("Length", |track: &Track| view! { <text body>{track.length}</text> }).flex(0.6)
             h=208.0/>
    };
    cell(
        "data_table",
        2.0,
        "Columns are closures over the row type. Rows come from a keyed list or a count, on virtual_list; sort, selection and widths are signals.",
        content,
    )
}

fn queue() -> Cell {
    let queue = keyed_list(|title: &&'static str| *title);
    queue.extend(ALBUMS.iter().take(4).map(|album| album.title));
    let content = view! {
        <sortable list=queue
             row={|title, _| {
                 view! {
                     <div flex items_center gap=8.0 h=28.0 px=8.0>
                         {Icon::new(IconName::AudioLines, Role::TextSecondary)}
                         <text body primary no_wrap>{*title}</text>
                     </div>
                 }
             }}
             test_id="queue"/>
    };
    cell(
        "sortable",
        1.0,
        "Drag a row, or focus it and press Alt with an arrow. Neighbours make room with Pace::Layout.",
        content,
    )
}

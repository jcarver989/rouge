use super::{band, cell, choice, compact, sheet};
use rouge::core::{
    Corners, Div, Fill, Mask, Role, Styled, div, dynamic, each, scroll, show, text, view,
};
use rouge::reactive::{Signal, memo, signal};
use rouge::widgets::tokens::{GROUP_HEIGHT, Pace, Radii, Radius, Space};
use rouge::widgets::{
    ButtonVariant, Icon, IconName, Material, MaterialExt, UiStyles, button, column_grid, grid_cell,
    icon_button, segmented_control, slider,
};

const TILE: f32 = 112.0;
const SIDEBAR_CORNERS: Corners = Corners {
    top_left: Radii::DEFAULT.px(Radius::Floating),
    top_right: 0.0,
    bottom_right: 0.0,
    bottom_left: Radii::DEFAULT.px(Radius::Floating),
};
const THUMBNAIL: f32 = 40.0;
const TRACK_SECONDS: f32 = 242.0;
const BARS: [f32; 3] = [0.125, 0.425, 0.725];

#[derive(Clone, Copy)]
enum Art {
    Sun,
    Bars,
    Moon,
    Quarters,
    Rings,
}

pub(crate) struct Album {
    pub(crate) title: &'static str,
    pub(crate) artist: &'static str,
    art: Art,
    colors: [u32; 3],
}

const fn album(title: &'static str, artist: &'static str, art: Art, colors: [u32; 3]) -> Album {
    Album {
        title,
        artist,
        art,
        colors,
    }
}

pub(crate) const ALBUMS: [Album; 12] = [
    album(
        "Soft Machinery",
        "Ines Okafor",
        Art::Rings,
        [0x4b2a7a, 0xf28b82, 0xfbd3c1],
    ),
    album(
        "Low Tide",
        "Ines Okafor",
        Art::Sun,
        [0x0b1d3a, 0x1259c3, 0x21d4fd],
    ),
    album(
        "Paper Lanterns",
        "Yuki Aoyama",
        Art::Bars,
        [0x5a1620, 0xf2a900, 0xffd166],
    ),
    album(
        "Second Summer",
        "The Understudies",
        Art::Moon,
        [0x2bb39a, 0x3d4fc4, 0xc9ffd9],
    ),
    album(
        "Night Bus",
        "Tomas Reis",
        Art::Quarters,
        [0xf5c6ee, 0x152a5c, 0x0f5fd7],
    ),
    album(
        "Ferry Lights",
        "Tomas Reis",
        Art::Rings,
        [0x1b2a6b, 0x58b4f5, 0xb388eb],
    ),
    album(
        "Glasswork",
        "Marguerite Vale",
        Art::Bars,
        [0x1f1b4d, 0x00c9c0, 0x7ef0e6],
    ),
    album(
        "Harmattan",
        "Kofi Mensah-Lund",
        Art::Sun,
        [0x2d0c1e, 0x9c1f5b, 0xff9f2e],
    ),
    album(
        "Brine",
        "Salt Choir",
        Art::Moon,
        [0x0aa3a0, 0x067a86, 0xb7f5ff],
    ),
    album(
        "Open Window",
        "Yuki Aoyama",
        Art::Quarters,
        [0xb8062e, 0xe4e0a8, 0xc3bd2a],
    ),
    album(
        "Quiet Engines",
        "The Understudies",
        Art::Rings,
        [0x0d3b66, 0x3f8efc, 0xd9c2ff],
    ),
    album(
        "Small Hours",
        "Marguerite Vale",
        Art::Sun,
        [0x121735, 0x3243b0, 0xff8fcf],
    ),
];

pub(crate) fn hero(status: Signal<String>) -> Div {
    let compact = compact();
    let facts = [
        ("Grid", "4 px unit, 12 columns, 48 px gutter"),
        ("Type", "Inter and Lilex, 10 presets"),
        ("Motion", "6 paces, every one a spring"),
        ("Materials", "4 glasses, pigment and surface"),
    ]
    .map(|(name, fact)| {
        view! {
            <div flex items_center justify_between gap=Space::X4 h=GROUP_HEIGHT border_t=1.0
                 border_color=Role::BorderSubtle>
                <text small muted no_wrap>{name}</text>
                <text small primary no_wrap>{fact}</text>
            </div>
        }
    });
    let title = view! {
        <text text_size={move || if compact() { 40.0 } else { 72.0 }}
              line_height={move || if compact() { 1.1 } else { 1.0 }} weight=600.0 primary>
            "Glass, pigment, surface."
        </text>
    };
    let lede = view! {
        <text text_size=18.0 line_height={28.0 / 18.0} secondary>
            "Rouge UI is three materials on a 4 px grid, moved by springs and drawn by the compositor. Everything on this page is the real component set."
        </text>
    };
    let heading = view! {
        <column_grid items_end gap_y=Space::X8>
            <grid_cell span=12>{title}</grid_cell>
            <grid_cell span=6>{lede}</grid_cell>
            <grid_cell span=2/>
            <grid_cell span=4>
                <div flex col>{..facts}</div>
            </grid_cell>
        </column_grid>
    };
    let hints = [
        "Change the sort. Tiles travel to their new places with a layout transition, and a second click mid-flight retargets them.",
        "Scroll the albums under the glass toolbars. The glass is the same Material::Glass every menu and popover uses.",
        "Light, Dark and the accent switch in the top bar swap the palette. Every colour here is a role read from the theme.",
    ]
    .map(|hint| cell("", 1.0, "", view! { <text body secondary>{hint}</text> }).plain());
    view! {
        <div flex col gap=Space::X12>
            {heading}
            {sheet(band([cell("stage", 1.0, "", window(status)).plain()]).into_iter().chain(band(hints)))}
        </div>
    }
}

fn window(status: Signal<String>) -> Div {
    let (playing, sort) = (signal(0usize), signal(0usize));
    let order = memo(move || {
        let mut order: Vec<usize> = (0..ALBUMS.len()).collect();
        match sort.get() {
            1 => order.sort_by_key(|&index| ALBUMS[index].title),
            2 => order.sort_by_key(|&index| (ALBUMS[index].artist, ALBUMS[index].title)),
            _ => {}
        }
        order
    });
    let play = move |index: usize| {
        playing.set(index);
        status.set(format!("Playing {}", ALBUMS[index].title));
    };
    let tiles = view! {
        <each items={move || order.get()} key={|index| *index} let:index>
            {tile(index.get_untracked(), playing, play)}
        </each>
    };
    let albums = view! {
        <scroll absolute inset=0.0>
            <div flex flex_wrap gap_x=20.0 gap_y=16.0 px=20.0 pt=80.0 pb=96.0>{tiles}</div>
        </scroll>
    };
    let compact = compact();
    view! {
        <div flex h=520.0 rounded=Radius::Floating border=1.0 border_color=Role::Border overflow_hidden
             test_id="stage">
            <show when={move || !compact()} children=sidebar/>
            <div relative flex_1 min_w=0.0>
                {albums}
                {toolbar(sort, play)}
                {player(playing)}
            </div>
        </div>
    }
}

fn sidebar() -> Div {
    let item = |icon: IconName, label: &'static str, count: &'static str, current: bool| {
        let row = view! {
            <div flex items_center gap=8.0 h=28.0 px=8.0 rounded=Radius::Inner>
                {Icon::new(icon, Role::TextSecondary)}
                <div flex_1><text body primary>{label}</text></div>
                <text small muted>{count}</text>
            </div>
        };
        if current {
            row.bg(Role::SurfaceSelected)
        } else {
            row.hover(|style| style.bg(Role::SurfaceHover))
        }
    };
    let light = || view! { <div size=12.0 rounded=Radius::Full bg=Role::Border/> };
    view! {
        <div flex col flex_none gap=4.0 w=200.0 p=8.0 bg=Role::Chrome border_r=1.0 border_color=Role::BorderSubtle
             rounded=SIDEBAR_CORNERS>
            <div flex gap=8.0 px=8.0 pt=8.0 pb=16.0>{light()}{light()}{light()}</div>
            {item(IconName::CircleDot, "Albums", "12", true)}
            {item(IconName::Timer, "Recently played", "", false)}
            {item(IconName::Pin, "Loved", "42", false)}
            <div px=8.0 pt=12.0 pb=4.0><text label muted>"Playlists"</text></div>
            {item(IconName::Folder, "Late night", "", false)}
            {item(IconName::Folder, "Long drives", "", false)}
            {item(IconName::Folder, "Sunday kitchen", "", false)}
            {item(IconName::Plus, "New playlist", "", false)}
        </div>
    }
}

fn toolbar(sort: Signal<usize>, play: impl Fn(usize) + Copy + 'static) -> Div {
    let pick = |index: usize, label| choice(sort, index, label, format!("sort-{index}"));
    let compact = compact();
    let shuffled = signal(0usize);
    let shuffle = move || {
        shuffled.update(|next| *next = (*next + 5) % ALBUMS.len());
        play(shuffled.get_untracked());
    };
    view! {
        <div absolute top=12.0 left=12.0 right=12.0 h=48.0 flex items_center gap=12.0 px=12.0
             material=Material::Glass>
            <segmented_control test_id="sort">
                {..[
                   pick(0, "Recent"),
                   pick(1, "Title"),
                   pick(2, "Artist"),
                ]}
            </segmented_control>
            <div flex_1/>
            <show when={move || !compact()}>
                <div flex items_center gap=8.0 h=28.0 w=240.0 px=8.0 rounded=Radius::Control bg=Role::SurfaceHover>
                    {Icon::new(IconName::Search, Role::TextMuted)}
                    <text body muted>"Search albums and commands"</text>
                </div>
            </show>
            <button test_id="shuffle" label="Shuffle" icon=IconName::Play variant=ButtonVariant::Primary
                    on_click=shuffle/>
        </div>
    }
}

fn player(playing: Signal<usize>) -> Div {
    let compact = compact();
    let (position, volume, paused) = (signal(72.0_f32), signal(70.0_f32), signal(false));
    let step =
        move |by: usize| move || playing.update(|index| *index = (*index + by) % ALBUMS.len());
    let time = move || {
        let seconds = position.get().max(0.0).round();
        format!("{}:{:02}", (seconds / 60.0).floor(), seconds % 60.0)
    };
    let toggle = view! {
        <div flex flex_none items_center justify_center size=36.0 rounded=Radius::Full bg=Role::Accent
             access_label={move || if paused.get() { "Play" } else { "Pause" }.to_owned()}
             test_id="play-pause">
            <dynamic>
                {
                   let icon = if paused.get() { IconName::Play } else { IconName::Pause };
                   Icon::new(icon, Role::AccentForeground)
                }
            </dynamic>
        </div>
    }
    .on_activate(move || paused.toggle());
    view! {
        <div absolute bottom=12.0 left=12.0 right=12.0 h=64.0 flex items_center gap=12.0 px=12.0
             material=Material::Glass>
            <dynamic>{art(&ALBUMS[playing.get()], THUMBNAIL)}</dynamic>
            <div flex col flex_1 min_w=0.0 max_w=148.0>
                <text body_strong primary no_wrap>{move || ALBUMS[playing.get()].title.to_owned()}</text>
                <text small secondary no_wrap>{move || ALBUMS[playing.get()].artist.to_owned()}</text>
            </div>
            <icon_button icon=IconName::ChevronLeft label="Previous" on_click={step(ALBUMS.len() - 1)}/>
            {toggle}
            <icon_button icon=IconName::ChevronRight label="Next" on_click={step(1)}/>
            <show when={move || !compact()}>
                <div flex flex_1 items_center gap=12.0>
                    <div flex_none w=32.0><text small secondary>{time}</text></div>
                    <slider value=position access_label="Position" range={(0.0, TRACK_SECONDS)} step=1.0 flex_1/>
                    {Icon::new(IconName::AudioLines, Role::TextSecondary)}
                    <slider value=volume access_label="Volume" range={(0.0, 100.0)} step=1.0 w=96.0/>
                </div>
            </show>
        </div>
    }
}

fn tile(index: usize, playing: Signal<usize>, play: impl Fn(usize) + 'static) -> Div {
    let album = &ALBUMS[index];
    let ink = move || {
        Fill::Role(if playing.get() == index {
            Role::Accent
        } else {
            Role::Text
        })
    };
    view! {
        <div flex col flex_none w=TILE layout_transition=Pace::Layout test_id={format!("album-{index}")}>
            {art(album, TILE)}
            <div pt=8.0><text body_strong no_wrap color=ink>{album.title}</text></div>
            <text small muted no_wrap>{album.artist}</text>
        </div>
    }
    .on_activate(move || play(index))
}

fn art(album: &Album, size: f32) -> Div {
    let [base, first, second] = album.colors;
    let cover = view! {
        <div relative flex_none size=size rounded=Radius::Control overflow_hidden mask=Mask::shape()
             bg=base/>
    };
    let disc = |diameter: f32, x: f32, y: f32, color: u32| {
        view! {
            <div absolute left={x * size} top={y * size} size={diameter * size} rounded=Radius::Full
                 bg=color/>
        }
    };
    let slab = |x: f32, y: f32, width: f32, height: f32, color: u32| {
        view! {
            <div absolute left={x * size} top={y * size} w={width * size} h={height * size} bg=color/>
        }
    };
    match album.art {
        Art::Sun => cover
            .child(disc(1.2, -0.1, 0.45, first))
            .child(disc(0.7, 0.15, 0.7, second)),
        Art::Bars => {
            cover.children(BARS.iter().enumerate().map(|(index, &x)| {
                slab(x, 0.0, 0.15, 1.0, if index == 1 { second } else { first })
            }))
        }
        Art::Moon => cover
            .child(slab(0.0, 0.62, 1.0, 0.38, first))
            .child(disc(0.26, 0.6, 0.16, second)),
        Art::Quarters => cover
            .child(slab(0.5, 0.0, 0.5, 0.5, first))
            .child(slab(0.0, 0.5, 0.5, 0.5, second)),
        Art::Rings => cover
            .child(disc(0.8, 0.1, 0.1, first))
            .child(disc(0.44, 0.28, 0.28, second))
            .child(disc(0.14, 0.43, 0.43, base)),
    }
}

use super::{Cell, band, cell, row, section};
use rouge::core::{Div, Styled, div, show, text, view};
use rouge::reactive::{Signal, signal};
use rouge::widgets::{
    ButtonVariant, Hint, IconName, MenuItem, NoticeTone, UiStyles, button, context_menu,
    icon_button, modal, muted_text, popover, toast,
};

pub(crate) fn overlays(status: Signal<String>) -> Div {
    section(
        "Overlays",
        "Menus, popovers, tooltips and modals are glass, and the only things that cast a shadow.",
        [band([menus(status), popovers(), modals(), toasts(status)])],
    )
}

fn menus(status: Signal<String>) -> Cell {
    let items = [
        MenuItem::label("Queue"),
        MenuItem::callback("Play next", move || status.set("Play next".into()))
            .icon(IconName::Play)
            .shortcut("secondary-enter"),
        MenuItem::callback("Add to playlist", move || status.set("Added".into()))
            .icon(IconName::Plus),
        MenuItem::label("Share"),
        MenuItem::callback("Copy link", move || status.set("Copied".into()))
            .icon(IconName::Copy)
            .shortcut("secondary-c"),
        MenuItem::Separator,
        MenuItem::callback("Remove from library", move || status.set("Removed".into()))
            .icon(IconName::X)
            .destructive(),
    ];
    let menu = view! {
        <context_menu items dropdown>
            <button test_id="menu" label="Album actions" icon=IconName::ChevronDown variant=ButtonVariant::Secondary/>
        </context_menu>
    };
    cell(
        "context_menu",
        1.0,
        "Scales in from its trigger. Group labels, shortcuts in a kbd and a destructive tone.",
        view! { <row>{menu}</row> },
    )
}

fn popovers() -> Cell {
    let details = signal(false);
    let card = view! {
        <popover open=details
             trigger={view! { <button label="Details" test_id="details" variant=ButtonVariant::Secondary/> }}>
            <div flex col gap=4.0 p=12.0 w=240.0>
                <text label primary>"Soft Machinery"</text>
                <muted_text>"Ines Okafor. 11 tracks, 43 minutes."</muted_text>
            </div>
        </popover>
    };
    let content = view! {
        <row>
            {card}
            <icon_button icon=IconName::Copy label="Copy link".to_owned()/>
            {Hint::tooltip(view! { <button label="Hover me"/> }, "Soft Machinery").content(|| {
                view! {
                    <div flex col gap=4.0 py=4.0 w=200.0>
                        <text label primary>"Soft Machinery"</text>
                        <muted_text>"Ines Okafor. 11 tracks, 43 minutes."</muted_text>
                    </div>
                }
            })}
        </row>
    };
    cell(
        "popover · tooltip",
        1.0,
        "Both scale in with Pace::Enter. A tooltip takes a shortcut or rich content.",
        content,
    )
}

fn modals() -> Cell {
    let open = signal(false);
    let close = move || open.set(false);
    let dialog = view! {
        <show when={move || open.get()}>
            <modal title="Remove 3 albums from your library?".to_owned() test_id="dialog"
                 hint="Downloads are deleted from this device." on_dismiss=close>
                <div flex justify_end gap=8.0 pt=8.0>
                    <button label="Keep albums" variant=ButtonVariant::Secondary on_click=close/>
                    <button label="Remove albums" variant=ButtonVariant::Destructive on_click=close/>
                </div>
            </modal>
        </show>
    };
    let content = view! {
        <div flex>
            <button label="Remove 3 albums" test_id="open-modal" variant=ButtonVariant::Secondary
                 on_click={move || open.set(true)}/>
        </div>
    };
    cell(
        "modal",
        1.0,
        "The panel scales in while the scrim fades and blurs the page.",
        content.child(dialog),
    )
}

fn toasts(status: Signal<String>) -> Cell {
    let saved = || {
        toast("Added to Late night")
            .tone(NoticeTone::Positive)
            .show();
    };
    let undo = move || {
        toast("3 albums removed")
            .detail("Downloads were deleted from this device.")
            .action("Undo", move || status.set("Restored 3 albums".into()))
            .show();
    };
    let failed = || {
        toast("Could not reach Kitchen speaker")
            .tone(NoticeTone::Error)
            .show();
    };
    let content = view! {
        <row>
            <button label="Saved" test_id="toast-saved" variant=ButtonVariant::Secondary on_click=saved/>
            <button label="With undo" test_id="toast-undo" variant=ButtonVariant::Secondary on_click=undo/>
            <button label="Failed" test_id="toast-failed" variant=ButtonVariant::Secondary on_click=failed/>
        </row>
    };
    cell(
        "toast",
        1.0,
        "A per-window queue in the overlay root. The stack fans out on hover and pauses its timers.",
        content,
    )
}

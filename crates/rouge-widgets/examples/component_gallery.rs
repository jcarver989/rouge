use rouge::{App, AppError, WindowOptions};
use rouge_core::{Styled, Theme, View, component, div, text, view};
use rouge_reactive::{Signal, signal};
use rouge_theme::UiPalette;
use rouge_widgets::{
    ButtonVariant, IconName, InterfaceSize, Material, MaterialExt, MenuItem, NoticeTone,
    SelectionState, Side, UiStyles, action_bar, bundled_fonts, button, context_menu, empty_state,
    inline_edit, notification, resizable_panel, section_header, segment, segmented_control,
    tree_row,
};

fn main() -> Result<(), AppError> {
    App::new()
        .fonts(bundled_fonts())
        .window(
            WindowOptions::default()
                .title("Rouge kit component gallery")
                .size(920.0, 680.0),
            gallery,
        )
        .run()
}

fn gallery() -> impl View {
    let theme = Theme::current();
    theme.set(UiPalette::default());
    let size = InterfaceSize::install(16.0);
    let status = signal(String::from("Choose a control"));
    let edit = signal(String::from("Editable text"));
    let width = signal(16.0);
    let count = view! { <text caption muted>"2"</text> };
    size.bind(view! {
        <div size_full flex col p=16.0 gap=12.0 bg=theme.canvas>
            <text heading primary>"Shared Rouge components"</text>
            <controls size status/>
            <div flex gap=8.0 flex_1>
                <resizable_panel edge=Side::Left size=width test_id="sidebar" min_size=10.0 max_size=28.0>
                    <div flex col size_full material=Material::Surface>
                        <section_header id="files" title="Files" expanded=true badge=count.into_any()/>
                        <tree_row id="src" label="src" expanded=true icon=IconName::FolderOpen/>
                        <tree_row id="lib" label="lib.rs" depth=1 icon=IconName::File state=SelectionState::Selected
                                  on_click={move || status.set("Selected lib.rs".into())}/>
                    </div>
                </resizable_panel>
                <div flex col p=12.0 flex_1 min_w=0.0 gap=10.0 material=Material::Floating>
                    <segmented_control id="view">
                        <segment id="unified" label="Unified" selected/>
                        <segment id="split" label="Split" icon=IconName::Columns2/>
                        <segment id="disabled-segment" label="Disabled" disabled/>
                    </segmented_control>
                    <notification message="Theme tokens and bundled typefaces" tone=NoticeTone::Info/>
                    <inline_edit value=edit on_commit={move |value: String| status.set(value)}/>
                    <empty_state icon=IconName::FileDiff title="No changes" detail="The working tree is clean"/>
                    <action_bar>
                        <text caption primary>{move || status.get()}</text>
                    </action_bar>
                </div>
            </div>
        </div>
    })
}

#[component]
fn controls(size: InterfaceSize, status: Signal<String>) -> impl View {
    let items = [
        MenuItem::callback("Larger", move || size.set(size.get() + 1.0)).icon(IconName::Plus),
        MenuItem::callback("Smaller", move || size.set(size.get() - 1.0)).icon(IconName::Minus),
        MenuItem::Separator,
        MenuItem::callback("Reset", move || size.set(16.0)),
    ];
    view! {
        <div flex gap=8.0>
            <button id="primary" label="Primary" variant=ButtonVariant::Primary keyboard_focusable
                    on_click={move || status.set("Primary selected".into())}/>
            <button id="secondary" label="Secondary" variant=ButtonVariant::Secondary keyboard_focusable
                    on_click={move || status.set("Secondary selected".into())}/>
            <button id="destructive" label="Destructive" variant=ButtonVariant::Destructive
                    on_click={move || status.set("Destructive selected".into())}/>
            <button id="disabled" label="Disabled" disabled/>
            <context_menu items dropdown><button id="menu" label="Interface size"/></context_menu>
        </div>
    }
}

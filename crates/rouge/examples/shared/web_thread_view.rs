use rouge::prelude::*;
use rouge::webview::{WebContent, web_page};
use rouge::widgets::tokens::{HAIRLINE, Radius, Space};
use rouge::widgets::{IconName, UiStyles, icon_button, web_card};

const ARTIFACT: &str = include_str!("artifact.html");
const SITE: &str = "https://example.com/";
const COLUMN: f32 = 720.0;

pub(crate) fn web_thread() -> impl View {
    let appearance = appearance();
    let palette = move || UiPalette::for_dark(appearance.get() == Appearance::Dark);
    let theme = Theme::provide(untrack(palette));
    effect(move || theme.set(palette()));
    let heard = signal(String::from("Nothing yet"));
    let pricing =
        web_page(WebContent::html(ARTIFACT)).on_message(move |message: String| heard.set(message));
    let thread = view! {
        <div flex col gap=Space::X4 w_full max_w=COLUMN px=Space::X4 py=Space::X8>
            {asked("Draft a pricing page with three plans and a monthly / yearly switch.")}
            {said("Here is a first pass. Click the page to try the switch; click outside it to come back to the thread.")}
            <web_card page=pricing name="pricing.html" icon=IconName::FileText tag="v3" test_id="pricing"
                 action={view! {
                     <icon_button icon=IconName::MessageCircle label="Send a note to the page".to_owned()
                          test_id="pricing-note"
                          on_click={move || pricing.post_message("Rouge says hello over the bridge.")}/>
                 }}/>
            {said("The page reports what you choose back to the app: the line above the thread updates when you press one of its buttons.")}
            {asked("Can you show me a live site in the same kind of card?")}
            {said("Any URL works. This one keeps its own history, and Reload asks the page to load again.")}
            <web_card page={web_page(WebContent::url(SITE))} name="example.com" test_id="site" page_size=(720.0, 450.0)
                 on_open={move || heard.set(format!("Would open {SITE} in the browser"))}/>
            {said("Scroll the thread with the pointer over either page: a sleeping page never takes the wheel.")}
            <div h=320.0 flex_none/>
        </div>
    };
    view! {
        <div size_full flex col bg=Role::Canvas>
            <div flex flex_none items_center gap=Space::X2 px=Space::X4 py=Space::X3 bg=Role::Chrome
                 border_b=HAIRLINE border_color=Role::BorderSubtle>
                <text title primary no_wrap>"Artifacts"</text>
                <div flex_1 min_w=0.0>
                    <text small muted truncate test_id="heard">{move || format!("Last from the page: {}", heard.get())}</text>
                </div>
            </div>
            <scroll_view flex_1 min_h=0.0>
                <scroll flex_1 min_h=0.0 test_id="thread">
                    <div flex col items_center>{thread}</div>
                </scroll>
            </scroll_view>
        </div>
    }
}

fn asked(message: &'static str) -> Div {
    view! {
        <div flex justify_end>
            <div max_w=480.0 px=Space::X3 py=Space::X2 rounded=Radius::Floating bg=Role::SurfaceSelected>
                <text reading primary>{message}</text>
            </div>
        </div>
    }
}

fn said(message: &'static str) -> Div {
    view! { <div><text reading secondary>{message}</text></div> }
}

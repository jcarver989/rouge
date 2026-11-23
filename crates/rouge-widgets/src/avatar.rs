use crate::tokens::{ROW_HEIGHT, Radius, TINT_ALPHA};
use crate::typography::UiStyles;
use crate::util::{TestId, chrome};
use rouge_core::access;
use rouge_core::{
    BuildCx, Color, DelegateStyle, Div, Fit, ImageSource, IntoProp, Prop, Role, StyleDelta, Styled,
    Theme, View, component, div, image, text, view,
};

pub struct Avatar {
    name: String,
    image: Option<Prop<ImageSource>>,
    size: f32,
    ringed: bool,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn avatar(#[prop(into)] name: String) -> Avatar {
    Avatar {
        name,
        image: None,
        size: ROW_HEIGHT,
        ringed: false,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl Avatar {
    pub fn image<T>(mut self, source: impl IntoProp<ImageSource, T>) -> Self {
        self.image = Some(source.into_prop());
        self
    }

    pub fn avatar_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

chrome!(Avatar);

impl View for Avatar {
    fn build(self, cx: &mut BuildCx<'_>) {
        let theme = Theme::current();
        let face = match self.image {
            Some(source) => view! {
                <image source=source fit=Fit::Cover size_full rounded=Radius::Full/>
            }
            .into_any(),
            None => view! { <text caption weight=600.0 no_wrap access_hidden=true>{initials(&self.name)}</text> }
                .into_any(),
        };
        let root = view! {
            <disc size=self.size ringed=self.ringed role=access::Role::Image
                 access_label={self.name.clone()} text_color=Role::Accent
                 bg={move || {
                     theme
                         .get()
                         .accent
                         .with_alpha(TINT_ALPHA)
                         .over(theme.get().canvas)
                 }}
                 test_id?=self.test_id>
                {face}
            </disc>
        };
        self.style.apply(root).build(cx);
    }
}

#[component]
fn disc(size: f32, ringed: bool) -> Div {
    view! {
        <div flex flex_none items_center justify_center size=size rounded=Radius::Full overflow_hidden
             when:border=(ringed, RING) border_color=Role::Canvas/>
    }
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

pub struct AvatarGroup {
    avatars: Vec<Avatar>,
    max: usize,
    test_id: Option<String>,
    style: StyleDelta,
}

#[component]
pub fn avatar_group(avatars: impl IntoIterator<Item = Avatar>) -> AvatarGroup {
    AvatarGroup {
        avatars: avatars.into_iter().collect(),
        max: 4,
        test_id: None,
        style: StyleDelta::default(),
    }
}

impl AvatarGroup {
    pub fn max(mut self, max: usize) -> Self {
        self.max = max.max(1);
        self
    }
}

chrome!(AvatarGroup);

impl View for AvatarGroup {
    fn build(self, cx: &mut BuildCx<'_>) {
        let hidden = self.avatars.len().saturating_sub(self.max);
        let size = self.avatars.first().map_or(ROW_HEIGHT, |first| first.size);
        let overflow_id = self.test_id.part("overflow");
        let overflow = (hidden > 0).then(|| {
            view! {
                <disc size=size ringed=true ml=OVERLAP bg=Role::Chrome text_color=Role::TextSecondary
                     test_id?=overflow_id>
                    <text caption weight=600.0 no_wrap>{format!("+{hidden}")}</text>
                </disc>
            }
        });
        let shown =
            self.avatars
                .into_iter()
                .take(self.max)
                .enumerate()
                .map(|(index, mut avatar)| {
                    avatar.ringed = true;
                    if index > 0 {
                        avatar.ml(OVERLAP)
                    } else {
                        avatar
                    }
                });
        let root = view! {
            <div flex flex_none items_center border_color=Color::TRANSPARENT test_id?=self.test_id>
                {..shown}
                {overflow}
            </div>
        };
        self.style.apply(root).build(cx);
    }
}

const RING: f32 = 2.0;
const OVERLAP: f32 = -4.0;

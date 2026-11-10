use crate::length::Length;
use crate::prop::{IntoProp, Prop};
use rouge_render::{Color, Corners, Point};
use std::collections::BTreeMap;
use taffy::{
    AlignItems, AlignSelf, Dimension, Display, FlexDirection, FlexWrap, JustifyContent, Overflow,
    Point as Axes, Position, Rect as Sides, Size as Extent, Style as Layout,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Dirty {
    #[default]
    None,
    Composite,
    Backing,
    Paint,
    Layout,
}

impl Dirty {
    pub fn needs_layout(self) -> bool {
        self == Dirty::Layout
    }

    pub fn needs_paint(self) -> bool {
        self >= Dirty::Paint
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub offset: Point,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

impl BoxShadow {
    pub const fn new(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        BoxShadow {
            offset: Point::new(offset_x, offset_y),
            blur,
            spread: 0.0,
            color,
        }
    }

    pub const fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub const fn sm() -> Self {
        BoxShadow::new(0.0, 1.0, 3.0, Color::new(0.0, 0.0, 0.0, 0.18))
    }

    pub const fn md() -> Self {
        BoxShadow::new(0.0, 4.0, 12.0, Color::new(0.0, 0.0, 0.0, 0.22))
    }

    pub const fn lg() -> Self {
        BoxShadow::new(0.0, 12.0, 32.0, Color::new(0.0, 0.0, 0.0, 0.28))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub layout: Layout,
    pub rem_lengths: BTreeMap<&'static str, f32>,
    pub text_color: Option<Color>,
    pub text_size: Option<Length>,
    pub text_font: Option<String>,
    pub text_weight: Option<f32>,
    pub text_italic: Option<bool>,
    pub text_line_height: Option<f32>,
    pub background: Option<Color>,
    pub border_color: Color,
    pub corner_radii: Corners,
    pub shadows: Vec<BoxShadow>,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            layout: Layout {
                display: Display::Block,
                ..Default::default()
            },
            rem_lengths: BTreeMap::new(),
            text_color: None,
            text_size: None,
            text_font: None,
            text_weight: None,
            text_italic: None,
            text_line_height: None,
            background: None,
            border_color: Color::BLACK,
            corner_radii: Corners::ZERO,
            shadows: Vec::new(),
        }
    }
}

impl Style {
    pub(crate) fn same_box(&self, other: &Self) -> bool {
        self.layout == other.layout
            && self.rem_lengths == other.rem_lengths
            && self.background == other.background
            && self.border_color == other.border_color
            && self.corner_radii == other.corner_radii
            && self.shadows == other.shadows
    }

    fn length(&mut self, value: Length, key: &'static str) -> Length {
        if let Length::Rem(value) = value {
            self.rem_lengths.insert(key, value);
        } else {
            self.rem_lengths.remove(key);
        }
        value
    }

    pub(crate) fn resolved_layout(&self, root: f32) -> Layout {
        let mut layout = self.layout.clone();
        macro_rules! resolve {
            ($($field:ident.$side:ident),*) => {$(
                if let Some(value) = self.rem_lengths.get(stringify!($field.$side)) {
                    layout.$field.$side = Length::Px(value * root).into();
                }
            )*};
        }
        resolve!(
            size.width,
            size.height,
            min_size.width,
            min_size.height,
            max_size.width,
            max_size.height,
            gap.width,
            gap.height,
            padding.top,
            padding.right,
            padding.bottom,
            padding.left,
            margin.top,
            margin.right,
            margin.bottom,
            margin.left,
            inset.top,
            inset.right,
            inset.bottom,
            inset.left,
            border.top,
            border.right,
            border.bottom,
            border.left
        );
        if let Some(value) = self.rem_lengths.get("flex_basis") {
            layout.flex_basis = Length::Px(value * root).into();
        }
        layout
    }
}

macro_rules! props {
    ($($name:ident($ty:ty) [$dirty:ident] |$s:ident, $v:ident| $body:expr;)*) => {$(
        fn $name<U>(self, value: impl IntoProp<$ty, U>) -> Self {
            self.prop(value, Dirty::$dirty, |$s: &mut Style, $v: $ty| $body)
        }
    )*};
}

macro_rules! sides {
    ($($field:ident: $all:ident $top:ident $right:ident $bottom:ident $left:ident;)*) => {$(
        props! {
            $all(Length) [Layout] |s, v| {
                s.layout.$field = Sides {
                    left: s.length(v, concat!(stringify!($field), ".left")).into(),
                    right: s.length(v, concat!(stringify!($field), ".right")).into(),
                    top: s.length(v, concat!(stringify!($field), ".top")).into(),
                    bottom: s.length(v, concat!(stringify!($field), ".bottom")).into(),
                };
            };
            $top(Length) [Layout] |s, v| s.layout.$field.top = s.length(v, concat!(stringify!($field), ".top")).into();
            $right(Length) [Layout] |s, v| s.layout.$field.right = s.length(v, concat!(stringify!($field), ".right")).into();
            $bottom(Length) [Layout] |s, v| s.layout.$field.bottom = s.length(v, concat!(stringify!($field), ".bottom")).into();
            $left(Length) [Layout] |s, v| s.layout.$field.left = s.length(v, concat!(stringify!($field), ".left")).into();
        }
    )*};
}

macro_rules! flags {
    ($($name:ident |$s:ident| $body:expr;)*) => {$(
        fn $name(mut self) -> Self {
            let $s = self.style_mut();
            $body;
            self
        }
    )*};
}

#[macro_export]
macro_rules! styled {
    ($name:ident $(<$generic:ident>)? => $field:tt) => {
        impl $(<$generic>)? $crate::Styled for $name $(<$generic>)? {
            fn style_mut(&mut self) -> &mut $crate::Style {
                $crate::Styled::style_mut(&mut self.$field)
            }

            fn prop<U: 'static, V>(
                mut self,
                value: impl $crate::IntoProp<U, V>,
                dirty: $crate::Dirty,
                set: fn(&mut $crate::Style, U),
            ) -> Self {
                self.$field = $crate::Styled::prop(self.$field, value, dirty, set);
                self
            }
        }
    };
}

pub trait Styled: Sized {
    fn style_mut(&mut self) -> &mut Style;

    fn prop<T: 'static, U>(
        self,
        value: impl IntoProp<T, U>,
        dirty: Dirty,
        set: fn(&mut Style, T),
    ) -> Self;

    fn style(mut self, style: Style) -> Self {
        *self.style_mut() = style;
        self
    }

    flags! {
        flex |s| s.layout.display = Display::Flex;
        block |s| s.layout.display = Display::Block;
        grid |s| s.layout.display = Display::Grid;
        row |s| s.layout.flex_direction = FlexDirection::Row;
        col |s| s.layout.flex_direction = FlexDirection::Column;
        flex_wrap |s| s.layout.flex_wrap = FlexWrap::Wrap;
        flex_1 |s| {
            s.rem_lengths.remove("flex_basis");
            s.layout.flex_grow = 1.0;
            s.layout.flex_shrink = 1.0;
            s.layout.flex_basis = Dimension::percent(0.0);
        };
        flex_none |s| {
            s.layout.flex_grow = 0.0;
            s.layout.flex_shrink = 0.0;
        };
        items_start |s| s.layout.align_items = AlignItems::FLEX_START;
        items_center |s| s.layout.align_items = AlignItems::CENTER;
        items_end |s| s.layout.align_items = AlignItems::FLEX_END;
        items_stretch |s| s.layout.align_items = AlignItems::STRETCH;
        justify_start |s| s.layout.justify_content = JustifyContent::FLEX_START;
        justify_center |s| s.layout.justify_content = JustifyContent::CENTER;
        justify_end |s| s.layout.justify_content = JustifyContent::FLEX_END;
        justify_between |s| s.layout.justify_content = JustifyContent::SPACE_BETWEEN;
        justify_around |s| s.layout.justify_content = JustifyContent::SPACE_AROUND;
        justify_evenly |s| s.layout.justify_content = JustifyContent::SPACE_EVENLY;
        self_start |s| s.layout.align_self = Some(AlignSelf::FLEX_START);
        self_center |s| s.layout.align_self = Some(AlignSelf::CENTER);
        self_end |s| s.layout.align_self = Some(AlignSelf::FLEX_END);
        self_stretch |s| s.layout.align_self = Some(AlignSelf::STRETCH);
        absolute |s| s.layout.position = Position::Absolute;
        relative |s| s.layout.position = Position::Relative;
        overflow_hidden |s| s.layout.overflow = Axes { x: Overflow::Hidden, y: Overflow::Hidden };
        w_full |s| {
            s.rem_lengths.remove("size.width");
            s.layout.size.width = Dimension::percent(1.0);
        };
        h_full |s| {
            s.rem_lengths.remove("size.height");
            s.layout.size.height = Dimension::percent(1.0);
        };
        size_full |s| {
            s.rem_lengths.remove("size.width");
            s.rem_lengths.remove("size.height");
            s.layout.size = Extent { width: Dimension::percent(1.0), height: Dimension::percent(1.0) };
        };
    }

    fn italic(self) -> Self {
        self.prop(true, Dirty::Layout, |style, value| {
            style.text_italic = Some(value);
        })
    }

    props! {
        text_color(Color) [Paint] |s, v| s.text_color = Some(v);
        text_size(Length) [Layout] |s, v| s.text_size = Some(v);
        font(String) [Layout] |s, v| s.text_font = Some(v);
        font_weight(f32) [Layout] |s, v| s.text_weight = Some(v);
        line_height(f32) [Layout] |s, v| s.text_line_height = Some(v);
        w(Length) [Layout] |s, v| s.layout.size.width = s.length(v, "size.width").into();
        h(Length) [Layout] |s, v| s.layout.size.height = s.length(v, "size.height").into();
        size(Length) [Layout] |s, v| s.layout.size = Extent { width: s.length(v, "size.width").into(), height: s.length(v, "size.height").into() };
        min_w(Length) [Layout] |s, v| s.layout.min_size.width = s.length(v, "min_size.width").into();
        min_h(Length) [Layout] |s, v| s.layout.min_size.height = s.length(v, "min_size.height").into();
        max_w(Length) [Layout] |s, v| s.layout.max_size.width = s.length(v, "max_size.width").into();
        max_h(Length) [Layout] |s, v| s.layout.max_size.height = s.length(v, "max_size.height").into();
        aspect_ratio(f32) [Layout] |s, v| s.layout.aspect_ratio = Some(v);

        flex_grow(f32) [Layout] |s, v| s.layout.flex_grow = v;
        flex_shrink(f32) [Layout] |s, v| s.layout.flex_shrink = v;
        flex_basis(Length) [Layout] |s, v| s.layout.flex_basis = s.length(v, "flex_basis").into();
        gap(Length) [Layout] |s, v| s.layout.gap = Extent { width: s.length(v, "gap.width").into(), height: s.length(v, "gap.height").into() };
        gap_x(Length) [Layout] |s, v| s.layout.gap.width = s.length(v, "gap.width").into();
        gap_y(Length) [Layout] |s, v| s.layout.gap.height = s.length(v, "gap.height").into();

        px(Length) [Layout] |s, v| {
            s.layout.padding.left = s.length(v, "padding.left").into();
            s.layout.padding.right = s.length(v, "padding.right").into();
        };
        py(Length) [Layout] |s, v| {
            s.layout.padding.top = s.length(v, "padding.top").into();
            s.layout.padding.bottom = s.length(v, "padding.bottom").into();
        };
        mx(Length) [Layout] |s, v| {
            s.layout.margin.left = s.length(v, "margin.left").into();
            s.layout.margin.right = s.length(v, "margin.right").into();
        };
        my(Length) [Layout] |s, v| {
            s.layout.margin.top = s.length(v, "margin.top").into();
            s.layout.margin.bottom = s.length(v, "margin.bottom").into();
        };

        border_color(Color) [Paint] |s, v| s.border_color = v;
        bg(Color) [Backing] |s, v| s.background = Some(v);
        rounded(Corners) [Backing] |s, v| s.corner_radii = v;
        shadow(BoxShadow) [Paint] |s, v| s.shadows = vec![v];
        shadows(Vec<BoxShadow>) [Paint] |s, v| s.shadows = v;
    }

    sides! {
        padding: p pt pr pb pl;
        margin: m mt mr mb ml;
        inset: inset top right bottom left;
        border: border border_t border_r border_b border_l;
    }
}

impl Styled for Style {
    fn style_mut(&mut self) -> &mut Style {
        self
    }

    fn prop<T: 'static, U>(
        mut self,
        value: impl IntoProp<T, U>,
        _dirty: Dirty,
        set: fn(&mut Style, T),
    ) -> Self {
        match value.into_prop() {
            Prop::Static(value) => set(&mut self, value),
            Prop::Dynamic(compute) => set(&mut self, compute()),
        }
        self
    }
}

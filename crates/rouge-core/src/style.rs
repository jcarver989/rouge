use crate::length::Length;
use crate::prop::{IntoProp, Prop};
use crate::tree::Variant;
use rouge_render::{Color, Corners, Point};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::rc::Rc;
use taffy::{
    AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, JustifyContent, Overflow,
    Point as Axes, Position, Style as Layout,
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

#[derive(Clone, Default)]
pub struct GroupStyles(pub(crate) Vec<(String, Variant)>);

impl Debug for GroupStyles {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_list()
            .entries(self.0.iter().map(|(name, _)| name))
            .finish()
    }
}

impl PartialEq for GroupStyles {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|((name, style), (other_name, other_style))| {
                    name == other_name && Rc::ptr_eq(style, other_style)
                })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub layout: Layout,
    pub group_styles: GroupStyles,
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
            group_styles: GroupStyles::default(),
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

    pub(crate) fn resolved_layout(&self, root: f32) -> Layout {
        let mut layout = self.layout.clone();
        for field in LayoutLength::ALL {
            if let Some(value) = self.rem_lengths.get(field.key()) {
                field.assign(&mut layout, Length::Px(value * root));
            }
        }
        layout
    }
}

macro_rules! layout_lengths {
    ($apply:ident) => {
        $apply! {
            Width: w => size.width;
            Height: h => size.height;
            MinWidth: min_w => min_size.width;
            MinHeight: min_h => min_size.height;
            MaxWidth: max_w => max_size.width;
            MaxHeight: max_h => max_size.height;
            Basis: flex_basis => flex_basis;
            GapX: gap_x => gap.width;
            GapY: gap_y => gap.height;
            PaddingTop: pt => padding.top;
            PaddingRight: pr => padding.right;
            PaddingBottom: pb => padding.bottom;
            PaddingLeft: pl => padding.left;
            MarginTop: mt => margin.top;
            MarginRight: mr => margin.right;
            MarginBottom: mb => margin.bottom;
            MarginLeft: ml => margin.left;
            InsetTop: top => inset.top;
            InsetRight: right => inset.right;
            InsetBottom: bottom => inset.bottom;
            InsetLeft: left => inset.left;
            BorderTop: border_t => border.top;
            BorderRight: border_r => border.right;
            BorderBottom: border_b => border.bottom;
            BorderLeft: border_l => border.left;
        }
    };
}

macro_rules! length_fields {
    ($($variant:ident: $setter:ident => $field:ident $(.$side:ident)?;)*) => {
        #[derive(Clone, Copy)]
        enum LayoutLength { $($variant,)* }

        impl LayoutLength {
            const ALL: &[Self] = &[$(Self::$variant,)*];

            fn key(self) -> &'static str {
                match self { $(Self::$variant => stringify!($field $(.$side)?),)* }
            }

            fn assign(self, layout: &mut Layout, value: Length) {
                match self { $(Self::$variant => layout.$field $(.$side)? = value.into(),)* }
            }

            fn set(self, style: &mut Style, value: Length) {
                if let Length::Rem(value) = value {
                    style.rem_lengths.insert(self.key(), value);
                } else {
                    style.rem_lengths.remove(self.key());
                }
                self.assign(&mut style.layout, value);
            }
        }
    };
}

layout_lengths!(length_fields);

macro_rules! text_properties {
    ($($field:ident: $ty:ty),* $(,)?) => {
        #[derive(Clone, Default)]
        pub(crate) struct TextProperties { $(pub $field: Option<$ty>,)* }

        impl TextProperties {
            pub(crate) fn inherit(&mut self, style: &Style) {
                $(if self.$field.is_none() { self.$field.clone_from(&style.$field); })*
            }

            pub(crate) fn cascade(&self, style: &Style) -> Self {
                Self { $($field: style.$field.as_ref().or(self.$field.as_ref()).cloned(),)* }
            }

            pub(crate) fn same(a: &Style, b: &Style) -> bool {
                $(a.$field == b.$field)&&*
            }
        }
    };
}

text_properties! {
    text_color: Color,
    text_size: Length,
    text_font: String,
    text_weight: f32,
    text_italic: bool,
    text_line_height: f32,
}

macro_rules! props {
    ($($name:ident($ty:ty) [$dirty:ident] |$s:ident, $v:ident| $body:expr;)*) => {$(
        fn $name<U>(self, value: impl IntoProp<$ty, U>) -> Self {
            self.prop(value, Dirty::$dirty, |$s: &mut Style, $v: $ty| $body)
        }
    )*};
}

macro_rules! length_setters {
    ($($variant:ident: $setter:ident => $field:ident $(.$side:ident)?;)*) => {
        props! { $($setter(Length) [Layout] |s, v| LayoutLength::$variant.set(s, v);)* }
    };
}

macro_rules! lengths {
    ($($setter:ident: $($field:ident),+;)*) => {
        props! { $($setter(Length) [Layout] |s, v| { $(LayoutLength::$field.set(s, v);)+ };)* }
    };
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
            LayoutLength::Basis.set(s, Length::Fraction(0.0));
            s.layout.flex_grow = 1.0;
            s.layout.flex_shrink = 1.0;
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
        w_full |s| LayoutLength::Width.set(s, Length::Fraction(1.0));
        h_full |s| LayoutLength::Height.set(s, Length::Fraction(1.0));
        size_full |s| {
            LayoutLength::Width.set(s, Length::Fraction(1.0));
            LayoutLength::Height.set(s, Length::Fraction(1.0));
        };
    }

    fn group_hover(
        mut self,
        name: impl Into<String>,
        f: impl Fn(Style) -> Style + 'static,
    ) -> Self {
        self.style_mut()
            .group_styles
            .0
            .push((name.into(), Rc::new(f)));
        self
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
        aspect_ratio(f32) [Layout] |s, v| s.layout.aspect_ratio = Some(v);
        flex_grow(f32) [Layout] |s, v| s.layout.flex_grow = v;
        flex_shrink(f32) [Layout] |s, v| s.layout.flex_shrink = v;

        border_color(Color) [Paint] |s, v| s.border_color = v;
        bg(Color) [Backing] |s, v| s.background = Some(v);
        rounded(Corners) [Backing] |s, v| s.corner_radii = v;
        shadow(BoxShadow) [Paint] |s, v| s.shadows = vec![v];
        shadows(Vec<BoxShadow>) [Paint] |s, v| s.shadows = v;
    }

    layout_lengths!(length_setters);

    lengths! {
        size: Width, Height;
        gap: GapX, GapY;
        px: PaddingLeft, PaddingRight;
        py: PaddingTop, PaddingBottom;
        mx: MarginLeft, MarginRight;
        my: MarginTop, MarginBottom;
        p: PaddingTop, PaddingRight, PaddingBottom, PaddingLeft;
        m: MarginTop, MarginRight, MarginBottom, MarginLeft;
        inset: InsetTop, InsetRight, InsetBottom, InsetLeft;
        border: BorderTop, BorderRight, BorderBottom, BorderLeft;
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

use crate::length::Length;
use crate::prop::{IntoProp, Prop};
use rouge_render::{Color, Corners, Point};
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
            background: None,
            border_color: Color::BLACK,
            corner_radii: Corners::ZERO,
            shadows: Vec::new(),
        }
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
                s.layout.$field = Sides { left: v.into(), right: v.into(), top: v.into(), bottom: v.into() };
            };
            $top(Length) [Layout] |s, v| s.layout.$field.top = v.into();
            $right(Length) [Layout] |s, v| s.layout.$field.right = v.into();
            $bottom(Length) [Layout] |s, v| s.layout.$field.bottom = v.into();
            $left(Length) [Layout] |s, v| s.layout.$field.left = v.into();
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
        w_full |s| s.layout.size.width = Dimension::percent(1.0);
        h_full |s| s.layout.size.height = Dimension::percent(1.0);
        size_full |s| {
            s.layout.size = Extent { width: Dimension::percent(1.0), height: Dimension::percent(1.0) };
        };
    }

    props! {
        w(Length) [Layout] |s, v| s.layout.size.width = v.into();
        h(Length) [Layout] |s, v| s.layout.size.height = v.into();
        size(Length) [Layout] |s, v| s.layout.size = Extent { width: v.into(), height: v.into() };
        min_w(Length) [Layout] |s, v| s.layout.min_size.width = v.into();
        min_h(Length) [Layout] |s, v| s.layout.min_size.height = v.into();
        max_w(Length) [Layout] |s, v| s.layout.max_size.width = v.into();
        max_h(Length) [Layout] |s, v| s.layout.max_size.height = v.into();
        aspect_ratio(f32) [Layout] |s, v| s.layout.aspect_ratio = Some(v);

        flex_grow(f32) [Layout] |s, v| s.layout.flex_grow = v;
        flex_shrink(f32) [Layout] |s, v| s.layout.flex_shrink = v;
        flex_basis(Length) [Layout] |s, v| s.layout.flex_basis = v.into();
        gap(Length) [Layout] |s, v| s.layout.gap = Extent { width: v.into(), height: v.into() };
        gap_x(Length) [Layout] |s, v| s.layout.gap.width = v.into();
        gap_y(Length) [Layout] |s, v| s.layout.gap.height = v.into();

        px(Length) [Layout] |s, v| {
            s.layout.padding.left = v.into();
            s.layout.padding.right = v.into();
        };
        py(Length) [Layout] |s, v| {
            s.layout.padding.top = v.into();
            s.layout.padding.bottom = v.into();
        };
        mx(Length) [Layout] |s, v| {
            s.layout.margin.left = v.into();
            s.layout.margin.right = v.into();
        };
        my(Length) [Layout] |s, v| {
            s.layout.margin.top = v.into();
            s.layout.margin.bottom = v.into();
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

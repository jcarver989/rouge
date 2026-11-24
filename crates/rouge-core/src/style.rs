use crate::length::Length;
use crate::prop::{IntoProp, Prop};
use crate::text::{TextStyle, TextWrap};
use crate::theme::Fill;
use crate::tree::Interaction;
use rouge_render::{Color, Corners, Point};
use std::borrow::Cow;
use std::mem::{replace, take};
use std::rc::Rc;
use taffy::style_helpers::{evenly_sized_tracks, line, span};
use taffy::{
    AlignItems, AlignSelf, Display, FlexDirection, FlexWrap, GridPlacement, JustifyContent, Line,
    Overflow, Point as Axes, Position, Style as Layout,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) enum Dirty {
    #[default]
    None,
    Composite,
    Paint,
    Layout,
}

impl Dirty {
    pub(crate) fn needs_layout(self) -> bool {
        self == Dirty::Layout
    }

    pub(crate) fn needs_paint(self) -> bool {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    Basis,
    GapX,
    GapY,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    Top,
    Right,
    Bottom,
    Left,
    BorderTop,
    BorderRight,
    BorderBottom,
    BorderLeft,
}

fn set_length(layout: &mut Layout, slot: Slot, length: Length) -> bool {
    match slot {
        Slot::Width => set(&mut layout.size.width, length.into()),
        Slot::Height => set(&mut layout.size.height, length.into()),
        Slot::MinWidth => set(&mut layout.min_size.width, length.into()),
        Slot::MinHeight => set(&mut layout.min_size.height, length.into()),
        Slot::MaxWidth => set(&mut layout.max_size.width, length.into()),
        Slot::MaxHeight => set(&mut layout.max_size.height, length.into()),
        Slot::Basis => set(&mut layout.flex_basis, length.into()),
        Slot::GapX => set(&mut layout.gap.width, length.into()),
        Slot::GapY => set(&mut layout.gap.height, length.into()),
        Slot::PaddingTop => set(&mut layout.padding.top, length.into()),
        Slot::PaddingRight => set(&mut layout.padding.right, length.into()),
        Slot::PaddingBottom => set(&mut layout.padding.bottom, length.into()),
        Slot::PaddingLeft => set(&mut layout.padding.left, length.into()),
        Slot::MarginTop => set(&mut layout.margin.top, length.into()),
        Slot::MarginRight => set(&mut layout.margin.right, length.into()),
        Slot::MarginBottom => set(&mut layout.margin.bottom, length.into()),
        Slot::MarginLeft => set(&mut layout.margin.left, length.into()),
        Slot::Top => set(&mut layout.inset.top, length.into()),
        Slot::Right => set(&mut layout.inset.right, length.into()),
        Slot::Bottom => set(&mut layout.inset.bottom, length.into()),
        Slot::Left => set(&mut layout.inset.left, length.into()),
        Slot::BorderTop => set(&mut layout.border.top, length.into()),
        Slot::BorderRight => set(&mut layout.border.right, length.into()),
        Slot::BorderBottom => set(&mut layout.border.bottom, length.into()),
        Slot::BorderLeft => set(&mut layout.border.left, length.into()),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Declaration {
    Display(Display),
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    FlexGrow(f32),
    FlexShrink(f32),
    AlignItems(AlignItems),
    AlignSelf(AlignSelf),
    JustifyContent(JustifyContent),
    Position(Position),
    Overflow(Overflow),
    AspectRatio(f32),
    GridColumns(u16),
    ColumnSpan(u16),
    ColumnStart(u16),
    Length(&'static [Slot], Length),
    TextColor(Fill),
    TextSize(f32),
    Font(Cow<'static, str>),
    FontWeight(f32),
    Italic(bool),
    Strikethrough(bool),
    LineHeight(f32),
    Background(Fill),
    BorderColor(Fill),
    Rounded(Corners),
    Shadows(Rc<[BoxShadow]>),
    Invisible(bool),
}

impl Declaration {
    pub(crate) fn is_layout(&self) -> bool {
        !matches!(
            self,
            Declaration::TextColor(_)
                | Declaration::TextSize(_)
                | Declaration::Font(_)
                | Declaration::FontWeight(_)
                | Declaration::Italic(_)
                | Declaration::Strikethrough(_)
                | Declaration::LineHeight(_)
                | Declaration::Background(_)
                | Declaration::BorderColor(_)
                | Declaration::Rounded(_)
                | Declaration::Shadows(_)
                | Declaration::Invisible(_)
        )
    }

    pub(crate) fn lay_out(&self, layout: &mut Layout) -> bool {
        match *self {
            Declaration::Display(display) => set(&mut layout.display, display),
            Declaration::FlexDirection(direction) => set(&mut layout.flex_direction, direction),
            Declaration::FlexWrap(wrap) => set(&mut layout.flex_wrap, wrap),
            Declaration::FlexGrow(grow) => set(&mut layout.flex_grow, grow),
            Declaration::FlexShrink(shrink) => set(&mut layout.flex_shrink, shrink),
            Declaration::AlignItems(align) => set(&mut layout.align_items, align),
            Declaration::AlignSelf(align) => set(&mut layout.align_self, Some(align)),
            Declaration::JustifyContent(justify) => set(&mut layout.justify_content, justify),
            Declaration::Position(position) => set(&mut layout.position, position),
            Declaration::Overflow(overflow) => set(
                &mut layout.overflow,
                Axes {
                    x: overflow,
                    y: overflow,
                },
            ),
            Declaration::AspectRatio(ratio) => set(&mut layout.aspect_ratio, Some(ratio)),
            Declaration::GridColumns(count) => set(
                &mut layout.grid_template_columns,
                evenly_sized_tracks(count),
            ),
            Declaration::ColumnSpan(count) => {
                let placed = Line {
                    start: layout.grid_column.start.clone(),
                    end: span(count),
                };
                set(&mut layout.grid_column, placed)
            }
            Declaration::ColumnStart(column) => {
                let start = i16::try_from(column)
                    .ok()
                    .filter(|column| *column > 0)
                    .map_or(GridPlacement::Auto, line);
                let placed = Line {
                    start,
                    end: layout.grid_column.end.clone(),
                };
                set(&mut layout.grid_column, placed)
            }
            Declaration::Length(slots, length) => slots.iter().fold(false, |changed, slot| {
                set_length(layout, *slot, length) | changed
            }),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Style {
    pub text_color: Option<Fill>,
    pub text_size: Option<f32>,
    pub text_font: Option<Cow<'static, str>>,
    pub text_weight: Option<f32>,
    pub text_italic: Option<bool>,
    pub text_strikethrough: Option<bool>,
    pub text_line_height: Option<f32>,
    pub background: Option<Fill>,
    pub border_color: Fill,
    pub corner_radii: Corners,
    pub shadows: Rc<[BoxShadow]>,
    pub invisible: bool,
    pub hidden: bool,
    pub clips: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            text_color: None,
            text_size: None,
            text_font: None,
            text_weight: None,
            text_italic: None,
            text_strikethrough: None,
            text_line_height: None,
            background: None,
            border_color: Fill::Color(Color::BLACK),
            corner_radii: Corners::ZERO,
            shadows: Rc::new([]),
            invisible: false,
            hidden: false,
            clips: false,
        }
    }
}

impl Style {
    pub(crate) fn apply(&mut self, declaration: &Declaration) {
        match declaration {
            Declaration::Display(display) => self.hidden = *display == Display::None,
            Declaration::Overflow(overflow) => self.clips = *overflow != Overflow::Visible,
            Declaration::TextColor(color) => self.text_color = Some(*color),
            Declaration::TextSize(size) => self.text_size = Some(*size),
            Declaration::Font(font) => self.text_font = Some(font.clone()),
            Declaration::FontWeight(weight) => self.text_weight = Some(*weight),
            Declaration::Italic(italic) => self.text_italic = Some(*italic),
            Declaration::Strikethrough(struck) => self.text_strikethrough = Some(*struck),
            Declaration::LineHeight(height) => self.text_line_height = Some(*height),
            Declaration::Background(color) => self.background = Some(*color),
            Declaration::BorderColor(color) => self.border_color = *color,
            Declaration::Rounded(radii) => self.corner_radii = *radii,
            Declaration::Shadows(shadows) => self.shadows = Rc::clone(shadows),
            Declaration::Invisible(invisible) => self.invisible = *invisible,
            _ => {}
        }
    }

    pub(crate) fn same_box(&self, other: &Self) -> bool {
        self.background == other.background
            && self.border_color == other.border_color
            && self.corner_radii == other.corner_radii
            && self.shadows == other.shadows
            && self.invisible == other.invisible
            && self.hidden == other.hidden
            && self.clips == other.clips
    }
}

fn set<T: PartialEq>(field: &mut T, value: T) -> bool {
    replace(field, value) != *field
}

pub(crate) fn base_layout() -> Layout {
    Layout {
        display: Display::Block,
        ..Layout::default()
    }
}

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
    text_color: Fill,
    text_size: f32,
    text_font: Cow<'static, str>,
    text_weight: f32,
    text_italic: bool,
    text_strikethrough: bool,
    text_line_height: f32,
}

macro_rules! props {
    ($($name:ident($ty:ty) => $declaration:expr;)*) => {$(
        fn $name<T>(self, value: impl IntoProp<$ty, T>) -> Self {
            self.declare(value.into_prop().map($declaration))
        }
    )*};
}

macro_rules! lengths {
    ($($name:ident: $($slot:ident),+;)*) => {
        props! { $($name(Length) => |length| Declaration::Length(&[$(Slot::$slot),+], length);)* }
    };
}

macro_rules! flags {
    ($($name:ident => $($declaration:expr),+;)*) => {$(
        fn $name(self) -> Self {
            self$(.declare(Prop::Static($declaration)))+
        }
    )*};
}

pub trait DelegateStyle {
    fn style_target(&mut self) -> &mut (impl Styled + Default);
}

impl<T: DelegateStyle> Styled for T {
    fn declare(mut self, declaration: Prop<Declaration>) -> Self {
        let target = self.style_target();
        *target = take(target).declare(declaration);
        self
    }

    fn restyle(mut self, state: StyleState, delta: StyleDelta) -> Self {
        let target = self.style_target();
        *target = take(target).restyle(state, delta);
        self
    }
}

pub trait Styled: Sized {
    fn declare(self, declaration: Prop<Declaration>) -> Self;

    #[must_use]
    fn restyle(self, state: StyleState, delta: StyleDelta) -> Self;

    fn hover(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.restyle(Interaction::Hover.into(), f(StyleDelta::default()))
    }

    fn active(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.restyle(Interaction::Active.into(), f(StyleDelta::default()))
    }

    fn selected(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.restyle(Interaction::Selected.into(), f(StyleDelta::default()))
    }

    fn checked(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.restyle(Interaction::Checked.into(), f(StyleDelta::default()))
    }

    fn focus(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.restyle(Interaction::Focus.into(), f(StyleDelta::default()))
    }

    fn focus_visible(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.restyle(Interaction::FocusVisible.into(), f(StyleDelta::default()))
    }

    fn disabled(self, f: impl FnOnce(StyleDelta) -> StyleDelta) -> Self {
        self.restyle(Interaction::Disabled.into(), f(StyleDelta::default()))
    }

    fn group_hover(
        self,
        name: impl Into<String>,
        f: impl FnOnce(StyleDelta) -> StyleDelta,
    ) -> Self {
        self.restyle(
            StyleState::GroupHover(name.into()),
            f(StyleDelta::default()),
        )
    }

    flags! {
        flex => Declaration::Display(Display::Flex);
        block => Declaration::Display(Display::Block);
        grid => Declaration::Display(Display::Grid);
        row => Declaration::FlexDirection(FlexDirection::Row);
        col => Declaration::FlexDirection(FlexDirection::Column);
        flex_wrap => Declaration::FlexWrap(FlexWrap::Wrap);
        flex_1 => Declaration::Length(&[Slot::Basis], Length::Fraction(0.0)),
            Declaration::FlexGrow(1.0),
            Declaration::FlexShrink(1.0);
        flex_none => Declaration::FlexGrow(0.0), Declaration::FlexShrink(0.0);
        items_start => Declaration::AlignItems(AlignItems::FLEX_START);
        items_center => Declaration::AlignItems(AlignItems::CENTER);
        items_end => Declaration::AlignItems(AlignItems::FLEX_END);
        items_stretch => Declaration::AlignItems(AlignItems::STRETCH);
        justify_start => Declaration::JustifyContent(JustifyContent::FLEX_START);
        justify_center => Declaration::JustifyContent(JustifyContent::CENTER);
        justify_end => Declaration::JustifyContent(JustifyContent::FLEX_END);
        justify_between => Declaration::JustifyContent(JustifyContent::SPACE_BETWEEN);
        justify_around => Declaration::JustifyContent(JustifyContent::SPACE_AROUND);
        justify_evenly => Declaration::JustifyContent(JustifyContent::SPACE_EVENLY);
        self_start => Declaration::AlignSelf(AlignSelf::FLEX_START);
        self_center => Declaration::AlignSelf(AlignSelf::CENTER);
        self_end => Declaration::AlignSelf(AlignSelf::FLEX_END);
        self_stretch => Declaration::AlignSelf(AlignSelf::STRETCH);
        absolute => Declaration::Position(Position::Absolute);
        relative => Declaration::Position(Position::Relative);
        overflow_hidden => Declaration::Overflow(Overflow::Hidden);
        invisible => Declaration::Invisible(true);
        visible => Declaration::Invisible(false);
        w_full => Declaration::Length(&[Slot::Width], Length::Fraction(1.0));
        h_full => Declaration::Length(&[Slot::Height], Length::Fraction(1.0));
        size_full => Declaration::Length(&[Slot::Width, Slot::Height], Length::Fraction(1.0));
        italic => Declaration::Italic(true);
    }

    fn typography(self, typography: impl Into<Typography>) -> Self {
        let typography = typography.into();
        let styled = self
            .text_size(typography.size)
            .line_height(typography.line_height)
            .font_weight(typography.weight);
        match typography.font {
            Some(font) => styled.font(font),
            None => styled,
        }
    }

    props! {
        text_color(Fill) => Declaration::TextColor;
        text_size(f32) => Declaration::TextSize;
        font(Cow<'static, str>) => Declaration::Font;
        font_weight(f32) => Declaration::FontWeight;
        line_height(f32) => Declaration::LineHeight;
        strikethrough(bool) => Declaration::Strikethrough;
        aspect_ratio(f32) => Declaration::AspectRatio;
        grid_cols(u16) => Declaration::GridColumns;
        col_span(u16) => Declaration::ColumnSpan;
        col_start(u16) => Declaration::ColumnStart;
        flex_grow(f32) => Declaration::FlexGrow;
        flex_shrink(f32) => Declaration::FlexShrink;
        border_color(Fill) => Declaration::BorderColor;
        bg(Fill) => Declaration::Background;
        rounded(Corners) => Declaration::Rounded;
        shadow(BoxShadow) => |shadow| Declaration::Shadows(Rc::new([shadow]));
        shadows(Vec<BoxShadow>) => |shadows: Vec<BoxShadow>| Declaration::Shadows(shadows.into());
    }

    lengths! {
        w: Width;
        h: Height;
        min_w: MinWidth;
        min_h: MinHeight;
        max_w: MaxWidth;
        max_h: MaxHeight;
        flex_basis: Basis;
        gap_x: GapX;
        gap_y: GapY;
        pt: PaddingTop;
        pr: PaddingRight;
        pb: PaddingBottom;
        pl: PaddingLeft;
        mt: MarginTop;
        mr: MarginRight;
        mb: MarginBottom;
        ml: MarginLeft;
        top: Top;
        right: Right;
        bottom: Bottom;
        left: Left;
        border_t: BorderTop;
        border_r: BorderRight;
        border_b: BorderBottom;
        border_l: BorderLeft;
        size: Width, Height;
        gap: GapX, GapY;
        px: PaddingLeft, PaddingRight;
        py: PaddingTop, PaddingBottom;
        mx: MarginLeft, MarginRight;
        my: MarginTop, MarginBottom;
        p: PaddingTop, PaddingRight, PaddingBottom, PaddingLeft;
        m: MarginTop, MarginRight, MarginBottom, MarginLeft;
        inset: Top, Right, Bottom, Left;
        border: BorderTop, BorderRight, BorderBottom, BorderLeft;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleState {
    Interaction(Interaction),
    GroupHover(String),
}

impl From<Interaction> for StyleState {
    fn from(interaction: Interaction) -> Self {
        StyleState::Interaction(interaction)
    }
}

#[derive(Default)]
pub struct StyleDelta {
    pub(crate) declarations: Vec<Prop<Declaration>>,
    pub(crate) states: Vec<(StyleState, StyleDelta)>,
}

impl Styled for StyleDelta {
    fn declare(mut self, declaration: Prop<Declaration>) -> Self {
        self.declarations.push(declaration);
        self
    }

    fn restyle(mut self, state: StyleState, delta: StyleDelta) -> Self {
        self.states.push((state, delta));
        self
    }
}

impl StyleDelta {
    pub fn apply<T: Styled>(self, target: T) -> T {
        let target = self.declarations.into_iter().fold(target, Styled::declare);
        self.states
            .into_iter()
            .fold(target, |target, (state, delta)| {
                target.restyle(state, delta)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    pub font: Option<&'static str>,
    pub size: f32,
    pub line_height: f32,
    pub weight: f32,
}

impl Typography {
    pub fn text_style(self) -> TextStyle {
        let defaults = TextStyle::default();
        TextStyle {
            family: self.font.map_or(defaults.family.clone(), Into::into),
            size: self.size,
            weight: self.weight,
            line_height: self.line_height,
            wrap: TextWrap::NoWrap,
            ..defaults
        }
    }
}

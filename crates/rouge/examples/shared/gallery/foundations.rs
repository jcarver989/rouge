use super::{COLUMN, Cell, band, cell, column, row, section};
use rouge::core::{
    Canvas, Color, Div, Ease, Fill, Glass, Path, Point, Rgba, Role, Spring, Styled, Theme, Timing,
    Tween, Typography, canvas, div, dynamic, text, view,
};
use rouge::reactive::{Signal, signal};
use rouge::widgets::tokens::{
    BAR_HEIGHT, CONTROL_HEIGHT_LARGE, GRID_GUTTER, GRID_MARGIN, GROUP_HEIGHT, ICON_BUTTON_SIZE,
    Pace, ROW_HEIGHT, Radius, Space, TINT_ALPHA,
};
use rouge::widgets::typography::{
    BODY, BODY_STRONG, CAPTION, CODE, DISPLAY, HEADING, LABEL, READING, SMALL, TITLE,
};
use rouge::widgets::{
    ButtonVariant, CODE_FONT, Material, MaterialExt, UiStyles, button, column_grid, grid_cell,
    segment, segmented_control, slider,
};

const SPECIMEN: f32 = 96.0;
const SWATCH: f32 = 48.0;
const SAMPLE: f32 = 144.0;
const LAB: f32 = 384.0;
const SLAB: (f32, f32) = (320.0, 192.0);
const CURVE: (f32, f32) = (160.0, 32.0);
const LANE: f32 = 160.0;
const PUCK: f32 = 16.0;
const STRIPE: f32 = 48.0;
const DARK_STRIPE: u32 = 0x1d2161;
const LIGHT_STRIPE: u32 = 0xd5e8de;
const SUN: u32 = 0xf0b93a;
const FROSTED: [f32; 4] = [10.0, 0.0, 0.0, 0.5];
const LENS: [f32; 4] = [6.0, 0.55, 0.06, 0.08];
const CLEAR: [f32; 4] = [0.0, 0.3, 0.0, 0.0];

pub(crate) fn foundations() -> Div {
    section(
        "Foundations",
        "One unit, one column grid, ten type presets and fourteen colour roles. Every distance is a step of the spacing scale and every block starts on a column. Turn on Grid in the top bar to check.",
        [band([grid(), sizes()]), band([typography(), roles()])],
    )
}

fn grid() -> Cell {
    let theme = Theme::current();
    let tint = move || theme.get().accent.with_alpha(TINT_ALPHA);
    let columns = (1..=8).map(move |number: u8| {
        view! {
            <div flex flex_1 items_end justify_center h=SPECIMEN pb=Space::X1 bg=tint>
                <text caption text_color=Role::Accent>{number.to_string()}</text>
            </div>
        }
    });
    let steps = Space::ALL.map(|space| {
        view! {
            <div flex col items_center gap=Space::X1>
                <div size={space.px()} bg=Role::Text/>
                <text caption muted>{format!("{}", space.px())}</text>
            </div>
        }
    });
    let content = view! {
        <column gap=Space::X6>
            <div flex gap=GRID_GUTTER>{..columns}</div>
            <div flex items_end gap=Space::X4>{..steps}</div>
        </column>
    };
    cell(
        "grid",
        2.0,
        "Columns at true size: these eight are the eight this cell spans. Cell dividers sit in the middle of a gutter, so the cell's margin puts content exactly on a column edge. The squares underneath are the whole spacing scale.",
        content,
    )
    .spec("12 columns, 48 gutter, 24 margin")
}

fn sizes() -> Cell {
    let heights = [
        (ICON_BUTTON_SIZE, "icon button, small control"),
        (ROW_HEIGHT, "row, control with a pointer"),
        (GROUP_HEIGHT, "group header, table row"),
        (BAR_HEIGHT, "bar"),
        (CONTROL_HEIGHT_LARGE, "control on touch"),
    ]
    .map(|(height, used)| {
        view! {
            <div flex items_center justify_between h=height px=Space::X3 rounded=Radius::Control
                 bg=Role::Elevated border=1.0 border_color=Role::Border>
                <text label primary>{format!("{height}")}</text>
                <text small muted no_wrap>{used}</text>
            </div>
        }
    });
    let radii = [
        (Radius::Inner, "Inner 4"),
        (Radius::Control, "Control 8"),
        (Radius::Floating, "Floating 12"),
        (Radius::Full, "Full"),
    ]
    .map(|(radius, name)| {
        view! {
            <div flex col gap=Space::X1>
                <div size=SWATCH rounded=radius bg=Role::SurfaceSelected border=1.0 border_color=Role::Border/>
                <text caption muted no_wrap>{name}</text>
            </div>
        }
    });
    let content = view! {
        <column gap=Space::X6>
            <column>{..heights}</column>
            <div flex gap=Space::X4>{..radii}</div>
        </column>
    };
    cell(
        "tokens",
        1.0,
        "A row inside a floating panel takes radius 8: the panel's 12 minus its 4 px inset, so the corners stay concentric.",
        content,
    )
    .spec("heights and radii")
}

fn typography() -> Cell {
    let presets: [(&str, Typography); 10] = [
        ("CAPTION", CAPTION),
        ("SMALL", SMALL),
        ("LABEL", LABEL),
        ("BODY", BODY),
        ("BODY_STRONG", BODY_STRONG),
        ("TITLE", TITLE),
        ("READING", READING),
        ("HEADING", HEADING),
        ("DISPLAY", DISPLAY),
        ("CODE", CODE),
    ];
    let rows = presets.map(|(name, preset)| {
        let measure = format!(
            "{} / {}, {}",
            preset.size,
            (preset.size * preset.line_height).round(),
            preset.weight
        );
        view! {
            <div flex items_center h=BAR_HEIGHT border_t=1.0 border_color=Role::BorderSubtle>
                <div w=COLUMN flex_none><text small font=CODE_FONT primary no_wrap>{name}</text></div>
                <div w=COLUMN flex_none><text small muted no_wrap>{measure}</text></div>
                <div flex_1 min_w=0.0 overflow_hidden>
                    <text typography=preset primary no_wrap>"Soft Machinery, side two"</text>
                </div>
            </div>
        }
    });
    cell(
        "typography",
        7.0,
        "Line heights are 16, 20, 24 and 28, so text lands on the unit however it wraps. Four weights, the four the crate bundles.",
        view! { <column gap=0.0>{..rows}</column> },
    )
    .spec("size / line, weight")
}

fn roles() -> Cell {
    let theme = Theme::current();
    let roles = [
        (Role::Canvas, "Canvas", "The page"),
        (Role::Surface, "Surface", "Sheets"),
        (Role::Chrome, "Chrome", "Wells and sidebars"),
        (Role::Elevated, "Elevated", "Glass tint, inputs"),
        (Role::Text, "Text", "Primary text"),
        (Role::TextSecondary, "TextSecondary", "Body and notes"),
        (Role::TextMuted, "TextMuted", "Hints"),
        (Role::Border, "Border", "Control edges"),
        (Role::BorderSubtle, "BorderSubtle", "Dividers"),
        (Role::Accent, "Accent", "Pigment"),
        (Role::AccentForeground, "AccentForeground", "Text on pigment"),
        (Role::Info, "Info", "Neutral notices"),
        (Role::Positive, "Positive", "Success"),
        (Role::Warning, "Warning", "Caution"),
        (Role::Destructive, "Destructive", "Danger"),
    ]
    .map(|(role, name, used)| {
        let against = if role == Role::AccentForeground {
            Role::Accent
        } else {
            Role::Canvas
        };
        let measured = view! {
            <dynamic>
                {
                    let canvas = theme.resolve(Fill::Role(Role::Canvas));
                    let behind = theme.resolve(Fill::Role(against)).over(canvas);
                    let ink = theme.resolve(Fill::Role(role)).over(behind);
                    let ratio = Rgba::from(ink).contrast(behind.into());
                    let label = if role == Role::Canvas {
                        String::new()
                    } else {
                        format!("{ratio:.1}")
                    };
                    view! { <text small primary no_wrap>{label}</text> }
                }
            </dynamic>
        };
        view! {
            <div flex items_center gap=Space::X2 h=GROUP_HEIGHT border_t=1.0 border_color=Role::BorderSubtle>
                <div size=16.0 flex_none rounded=Radius::Inner bg=role border=1.0 border_color=Role::Border/>
                <div w={COLUMN + GRID_MARGIN.px()} flex_none>
                    <text small font=CODE_FONT primary no_wrap>{name}</text>
                </div>
                <div flex_1 min_w=0.0 overflow_hidden><text small muted no_wrap>{used}</text></div>
                {measured}
            </div>
        }
    });
    cell(
        "Role",
        5.0,
        "Contrast is measured from the current palette, so it follows the appearance and accent switches. AccentForeground is measured on the accent.",
        view! { <column gap=0.0>{..roles}</column> },
    )
    .spec("contrast against canvas")
}

pub(crate) fn materials() -> Div {
    let sample = |material: Material, name: &'static str, detail: &'static str| {
        let slab = view! {
            <div flex col justify_end gap=Space::X1 p=Space::X3 h=SAMPLE material=material>
                <text label>{name}</text>
                <text small>{detail}</text>
            </div>
        };
        let slab = if material == Material::Pigment {
            slab.rounded(Radius::Floating)
        } else {
            slab.text_color(Role::Text)
        };
        view! { <grid_cell span=2>{slab}</grid_cell> }
    };
    let samples = view! {
        <column_grid relative py=Space::X12>
            {sample(
                Material::Glass,
                "Glass",
                "Menus, popovers, toolbars, sheets",
            )}
            {sample(
                Material::GlassControl,
                "GlassControl",
                "Control radius, no shadow",
            )}
            {sample(Material::Lens, "Lens", "Bends and splits its edge")}
            {sample(Material::Clear, "Clear", "No tint, no blur")}
            {sample(
                Material::Pigment,
                "Pigment",
                "The accent, flat and matte",
            )}
            {sample(
                Material::Surface,
                "Surface",
                "Opaque, in the page's flow",
            )}
        </column_grid>
    };
    let field = view! {
        <div relative overflow_hidden>
            {backdrop()}
            {samples}
        </div>
    };
    section(
        "Materials",
        "Glass for what floats, pigment for what you act on, surface for what sits in the page. Glass is the only one with depth, and only floating glass casts a shadow.",
        [band([cell("Material", 1.0, "", field).bleed()]), lab()],
    )
}

fn backdrop() -> Canvas {
    let theme = Theme::current();
    view! {
        <canvas
             draw={move |painter| {
                 let size = painter.size();
                 let whole = Path::builder()
                     .move_to(0.0, 0.0)
                     .line_to(size.width, 0.0)
                     .line_to(size.width, size.height)
                     .line_to(0.0, size.height)
                     .close()
                     .build();
                 painter.fill(&whole, Color::rgb(LIGHT_STRIPE));
                 let lean = size.height * 0.45;
                 let mut x = -lean;
                 while x < size.width {
                     let stripe = Path::builder()
                         .move_to(x + lean, 0.0)
                         .line_to(x + lean + STRIPE / 2.0, 0.0)
                         .line_to(x + STRIPE / 2.0, size.height)
                         .line_to(x, size.height)
                         .close()
                         .build();
                     painter.fill(&stripe, Color::rgb(DARK_STRIPE));
                     x += STRIPE;
                 }
                 let disc = |x: f32, y: f32, radius: f32| {
                     Path::builder()
                         .circle(Point::new(size.width * x, size.height * y), radius)
                         .build()
                 };
                 painter.fill(&disc(0.12, 0.3, 96.0), theme.get().accent);
                 painter.fill(&disc(0.86, 0.78, 128.0), Color::rgb(SUN));
             }}
             element={|div| div.absolute().inset(0.0)}/>
    }
}

fn lab() -> Vec<Div> {
    let theme = Theme::current();
    let [blur, refraction, dispersion, tint] = FROSTED.map(signal);
    let knobs = [blur, refraction, dispersion, tint];
    let (x, y) = (signal(0.0), signal(0.0));
    let slab = view! {
        <div test_id="glass-slab" w=SLAB.0 h=SLAB.1 rounded=Radius::Floating
             backdrop={move || {
                 Glass::new()
                     .blur(blur.get())
                     .refraction(refraction.get())
                     .dispersion(dispersion.get())
                     .tint(theme.get().elevated.with_alpha(tint.get()))
                     .rim_light(0.0, 0.0)
             }}
             transition=Pace::Layout translate_x={move || x.get()} translate_y={move || y.get()}
             on_drag={move |event| {
                 x.update(|x| *x += event.delta.x);
                 y.update(|y| *y += event.delta.y);
             }}/>
    };
    let stage = view! {
        <div relative flex flex_1 items_center justify_center min_h=LAB overflow_hidden>
            {backdrop()}
            {slab}
        </div>
    };
    let preset = move |name: &'static str, recipe: [f32; 4]| {
        view! {
            <segment label=name.to_owned() test_id={format!("glass-{}", name.to_lowercase())}
                 selected={move || knobs.map(|knob| knob.get()) == recipe}
                 on_click={move || {
                     for (knob, value) in knobs.into_iter().zip(recipe) {
                         knob.set(value);
                     }
                 }}/>
        }
    };
    let knob = |name: &'static str, value: Signal<f32>, most: f32, step: f32| {
        let shown = view! {
            <dynamic><text small secondary no_wrap>{format!("{:.2}", value.get())}</text></dynamic>
        };
        view! {
            <div flex items_center gap=Space::X3 h=ROW_HEIGHT>
                <div w=COLUMN flex_none><text body primary>{name}</text></div>
                <slider value=value test_id={format!("glass-{}", name.to_lowercase())} range={(0.0, most)}
                     step=step flex_1/>
                <div flex justify_end w=40.0 flex_none>{shown}</div>
            </div>
        }
    };
    let code = view! {
        <dynamic>
            {
                let source = format!(
                    "Glass::new()\n    .blur({:.1})\n    .refraction({:.2})\n    .dispersion({:.2})\n    .tint(elevated.with_alpha({:.2}))",
                    blur.get(),
                    refraction.get(),
                    dispersion.get(),
                    tint.get()
                );
                view! { <text small font=CODE_FONT secondary>{source}</text> }
            }
        </dynamic>
    };
    let controls = view! {
        <column gap=Space::X1>
            <div flex pb=Space::X3>
                <segmented_control test_id="glass-preset">
                    {..[
                        preset("Frosted", FROSTED),
                        preset("Lens", LENS),
                        preset("Clear", CLEAR),
                    ]}
                </segmented_control>
            </div>
            {knob("Blur", blur, 40.0, 1.0)}
            {knob("Refraction", refraction, 1.0, 0.01)}
            {knob("Dispersion", dispersion, 0.2, 0.01)}
            {knob("Tint", tint, 1.0, 0.01)}
            <div mt=Space::X3 p=Space::X3 rounded=Radius::Control bg=Role::Chrome>{code}</div>
        </column>
    };
    band([
        cell("Glass", 2.0, "", stage).bleed(),
        cell(
            "Glass",
            1.0,
            "Drag the slab. The three presets are the recipes behind Material::Glass, Lens and Clear; switching springs every uniform in the compositor. The rim is a fixed overhead light and nothing follows the pointer.",
            controls,
        )
        .spec("backdrop shader"),
    ])
}

pub(crate) fn motion() -> Div {
    section(
        "Motion",
        "A component picks a pace the way it picks a colour role. Each pace is a spring, so it can be interrupted and keeps its velocity. The curves are computed from the stiffness and damping in tokens.rs.",
        [band([paces(), retargeting()])],
    )
}

fn paces() -> Cell {
    let theme = Theme::current();
    let heading = view! {
        <div flex items_center h=ICON_BUTTON_SIZE>
            <div w=COLUMN flex_none><text small muted>"Pace"</text></div>
            <div w={CURVE.0 + GRID_GUTTER.px()} flex_none><text small muted>"Response"</text></div>
            <div w=COLUMN flex_none><text small muted>"Spring"</text></div>
            <div w=COLUMN flex_none><text small muted>"Settles"</text></div>
        </div>
    };
    let rows = [
        Pace::Press,
        Pace::Move,
        Pace::Enter,
        Pace::Exit,
        Pace::Layout,
        Pace::Gesture,
    ]
    .map(|pace| {
        let spring = pace.spring();
        let (samples, settles) = response(spring);
        let far = signal(false);
        let curve = view! {
            <canvas
                 draw={move |painter| {
                     let size = painter.size();
                     let level = |value: f32| size.height - 6.0 - value * (size.height - 14.0);
                     let target = Path::builder()
                         .move_to(0.0, level(1.0))
                         .line_to(size.width, level(1.0))
                         .build();
                     painter.stroke(&target, 1.0, theme.get().border);
                     let step = size.width / (samples.len() - 1) as f32;
                     let path = samples
                         .iter()
                         .enumerate()
                         .fold(Path::builder().move_to(0.0, level(0.0)), |path, (index, value)| {
                             path.line_to(index as f32 * step, level(*value))
                         })
                         .build();
                     painter.stroke_round(&path, 1.5, theme.get().accent);
                 }}
                 element={|div| div.w(CURVE.0).h(CURVE.1).flex_none()}/>
        };
        view! {
            <div flex items_center h={BAR_HEIGHT + Space::X2.px()} border_t=1.0 border_color=Role::BorderSubtle
                 test_id={format!("pace-{pace:?}").to_lowercase()} on_click={move |_| far.toggle()}>
                <div w=COLUMN flex_none><text small font=CODE_FONT primary no_wrap>{format!("Pace::{pace:?}")}</text></div>
                <div w={CURVE.0 + GRID_GUTTER.px()} flex_none>{curve}</div>
                <div w=COLUMN flex_none>
                    <text small muted no_wrap>{format!("{}, {}", spring.stiffness, spring.damping)}</text>
                </div>
                <div w=COLUMN flex_none><text small muted no_wrap>{format!("{:.0} ms", settles * 1000.0)}</text></div>
                {lane(pace, far)}
            </div>
        }
    });
    cell(
        "tokens::Pace",
        7.0,
        "MotionScale::None in context turns all six into instant changes for reduced motion.",
        view! {
            <column gap=0.0>
                {heading}
                {..rows}
            </column>
        },
    )
    .spec("click a row to run it")
}

fn lane(timing: impl Into<Timing>, far: Signal<bool>) -> Div {
    view! {
        <div w=LANE h=PUCK flex_none rounded=Radius::Full bg=Role::SurfaceHover>
            <div size=PUCK rounded=Radius::Full bg=Role::Accent transition={timing.into()}
                 translate_x={move || if far.get() { LANE - PUCK } else { 0.0 }}/>
        </div>
    }
}

fn response(spring: Spring) -> (Vec<f32>, f32) {
    const STEP: f32 = 0.00025;
    const STEPS: usize = 2400;
    const EVERY: usize = 40;
    let (mut value, mut velocity, mut settles) = (0.0_f32, 0.0_f32, 0.0_f32);
    let mut samples = Vec::with_capacity(STEPS / EVERY + 1);
    for index in 0..=STEPS {
        if index % EVERY == 0 {
            samples.push(value);
        }
        velocity +=
            (spring.stiffness * (1.0 - value) - spring.damping * velocity) / spring.mass * STEP;
        value += velocity * STEP;
        if (value - 1.0).abs() > 0.002 {
            settles = index as f32 * STEP;
        }
    }
    (samples, settles)
}

fn retargeting() -> Cell {
    let far = signal(false);
    let labelled = |label: &'static str, lane: Div| {
        view! {
            <div flex col gap=Space::X1>
                <text small muted>{label}</text>
                {lane}
            </div>
        }
    };
    let content = view! {
        <column gap=Space::X4>
            <row>
                <button label="Move".to_owned() test_id="play" variant=ButtonVariant::Primary
                     on_click={move || far.toggle()}/>
            </row>
            {labelled(
                "Spring, retargeted in flight",
                lane(Pace::Layout, far),
            )}
            {labelled(
                "Ease-out tween, 350 ms",
                lane(Tween::ms(350.0).ease(Ease::Out), far),
            )}
        </column>
    };
    cell(
        "retargeting",
        5.0,
        "Press Move, then again before it settles. The spring carries its velocity into the new target; the tween restarts from rest.",
        content,
    )
    .spec("click twice, quickly")
}

# Rouge

Rouge is a cross-platform UI framework for Rust whose goal is to be "write once; run anywhere". It uses the GPU to render the UI via [wgpu](https://wgpu.rs) and currently supports MacOS, Linux and web browsers. 

Rouge is heavily inspired by [Leptos](https://leptos.dev/) and [GPUI](https://gpui.rs/). It uses signals for fine-grained state tracking, is GPU accelerated, and supports compositing, shaders, animations and effects.

> **Status:** Rouge is in early development. The API can change at any time.

## Why Rouge?

- **Fast updates.** A component function runs one time. When a signal changes, Rouge changes only the properties that read it. Rouge does not re-render and does not diff.
- **No work when idle.** When nothing changes, Rouge draws no frames.
- **Smooth animation.** Springs, tweens and keyframes run in the compositor. They do not cause layout or paint. Rouge has a target of 120 Hz.
- **Effects are built in.** You can use blur, backdrop blur, liquid glass, shadows, masks and blend modes. You can also write your own effects in WGSL.
- **One codebase for three platforms.** The same code runs on macOS, on Linux and in the browser (WebAssembly with WebGPU, or WebGL2 where WebGPU is missing).
- **Two ways to write views.** Use Rust builder chains, or use JSX-style markup with `view!`. Both make the same calls, so you can use them together.
- **A full set of widgets.** Rouge has buttons, text inputs, menus, popovers, tabs, modals, sheets, sliders and more. For phones it adds navigation bars and stacks, grouped lists, search fields, chips, pagers, wheel pickers, alerts and a floating action button. On a phone-sized touch window the same popovers, menus and modals present as bottom sheets, tabs become a floating tab bar, sidebars become drawers and controls grow to touch size. Markdown and syntax highlighting are optional features.
- **Easy to test.** `TestUi` runs your UI without a window. A test can click, type and press keys, and then examine the result.

## Quick start

Rouge needs Rust 1.100 or later. Add it to your `Cargo.toml`:

```toml
[dependencies]
rouge = "0.1"
```

Put this code in `src/main.rs`:

```rust
use rouge::prelude::*;
use rouge::widgets::bundled_fonts;

fn counter() -> impl View {
    let count = signal(0);

    div()
        .flex()
        .col()
        .items_center()
        .gap(12.0)
        .p(24.0)
        .child(text(move || format!("Count: {}", count.get())).text_size(24.0))
        .child(button("+ 1").on_click(move || count.update(|c| *c += 1)))
        .child(show(move || count.get() >= 5, || text("That is a lot of clicks.")))
}

fn main() -> Result<(), rouge::AppError> {
    App::new()
        .fonts(bundled_fonts())
        .window(WindowOptions::default().title("Counter"), counter)
        .run()
}
```

Then run it:

```sh
cargo run
```

### How it works

1. `counter` runs one time. It makes the UI tree.
2. `signal(0)` makes a value that the UI can watch.
3. The closure in `text` reads `count`. When `count` changes, Rouge runs only this closure again and changes only this text.
4. `show` adds its child when the condition is true. It removes the child when the condition is false.

## Examples

### Use markup

This view is the same as the counter above. `#[component]` lets you use a function as a tag in `view!`.

```rust
#[component]
fn counter() -> impl View {
    let count = signal(0);

    view! {
        <div flex col items_center gap=12.0 p=24.0>
            <text text_size=24.0>"Count: {count}"</text>
            <button label="+ 1" on_click={move || count.update(|c| *c += 1)}/>
            <show when={move || count.get() >= 5}>
                <text>"That is a lot of clicks."</text>
            </show>
        </div>
    }
}
```

### Add a glass effect

A backdrop effect changes the content behind an element.

```rust
div()
    .p(16.0)
    .rounded(Corners::all(16.0))
    .backdrop(Glass::new().blur(24.0).refraction(0.4))
    .child(text("Frosted glass"))
```

### Brand shared widgets

Install `UiStyle` inside the window's root view, before constructing its children. For example, `UiStyle { radii: Radii::SHARP, materials: Materials::Solid, ..UiStyle::default() }.provide()` gives shared panels square corners and opaque surfaces. Set `fonts.ui` and `fonts.code` to family names registered with `App::fonts`. The style is chosen at mount; the palette and a `floating_panel`’s emphasis remain reactive. Without a provider, widgets use the original rounded glass style and bundled font families.

### Keep data between launches

Turn on the `persist` feature:

```toml
rouge = { version = "0.1", features = ["persist"] }
```

Give each window a storage, then wrap a signal or a keyed list in `persisted`. On macOS and Linux, Rouge keeps the data in files. In the browser, Rouge keeps the data in `localStorage`.

```rust
App::new().window(WindowOptions::default(), || {
    provide_storage(app_storage("My App"));
    todo_app()
})
```

```rust
let todos = persisted("todos", keyed_list(|todo: &Todo| todo.id));
let remaining = memo(move || todos.with_values(|all| all.iter().filter(|t| !t.done).count()));
```

### Test a view

`component_test()` makes a `TestUi` with the bundled fonts. It does not open a window.

```rust
use rouge::core::testing::TestResult;
use rouge::prelude::*;
use rouge::widgets::testing::component_test;

#[test]
fn the_button_adds_one() -> TestResult {
    let _runtime = Runtime::new().enter();
    let count = signal(0);
    let mut ui = component_test().build(move || {
        button("+ 1")
            .test_id("add")
            .on_click(move || count.update(|c| *c += 1))
    });

    ui.click_on("add")?;

    assert_eq!(count.get(), 1);
    Ok(())
}
```

## Run the examples

Clone this repository and install [Mise](https://mise.jdx.dev). From the checkout, run `mise trust` and `mise install`, then use `mise run run <example> [platform]`. Omit the platform to run on your macOS or Linux desktop.

```sh
mise run examples            # list examples and supported platforms
mise run run todomvc         # desktop
mise run run todomvc web     # browser
mise run run todomvc ios     # iOS simulator
mise run run smoke android   # connected arm64 Android device or emulator
```

| Example | What it shows | Platforms |
|---|---|---|
| `todomvc` | A full TodoMVC app with routes and saved data | Desktop, web, iOS |
| `gallery` | All of the widgets | Desktop, web, iOS |
| `counter` | The counter | Desktop |
| `glass` | Liquid glass | Desktop |
| `effects` | Blur, masks, blend modes and a custom WGSL effect | Desktop |
| `motion` | Springs, tweens and keyframes | Desktop |
| `diff` | A sample diff with syntax highlighting | Desktop |
| `settings` | A settings screen | Desktop |
| `surface` | GPU content from a different thread, shown with no copy | Desktop |
| `smoke` | A form with two text fields, a sheet and a list, packaged by `rouge-framer` | Android |

Use `mise tasks` to list workspace tasks, including the `platform:web:*`, `platform:ios:*`, and `platform:android:*` build and check tasks and the `perf:*` benchmark and profiling tasks. The web and iOS `build` tasks take Cargo example names (such as `component_gallery`); `mise run run` takes the IDs in the table above.

### In the browser

Install Python 3, the `wasm32-unknown-unknown` Rust target, and `wasm-bindgen-cli` at the same version as `wasm-bindgen` in `Cargo.lock`. After `mise run run todomvc web` starts the server, open <http://localhost:8000> in a browser that supports WebGPU or WebGL2. Add `?rouge_backend=gl` to the URL to use WebGL2 where WebGPU is available.

To ship a smaller Markdown app, disable default features on `rouge` and enable `markdown`. Add `rouge-syntax` with `default-features = false` and only the grammar features you need (for example `lang-rust`). Other dependencies can still enable `all-languages` through Cargo feature unification. Web builds use `wasm-opt` and Brotli when installed; to serve `.br` files, configure the host to return `Content-Encoding: br` with the original asset’s content type.

### In the iOS simulator

On an Apple Silicon Mac, select the full Xcode installation with `xcode-select`, install an iOS simulator runtime and the `aarch64-apple-ios-sim` Rust target, then run `mise run run todomvc ios`. This builds, installs, and launches the app and streams its console output. The default simulator is `iPhone 17 Pro`; set `ROUGE_IOS_DEVICE` to a name or UDID from `xcrun simctl list devices available` to use another one.

### On Android

Android support is experimental and needs API level 30 or later. Rouge runs inside a small generated `GameActivity`, so an Android app is packaged with Gradle through [`rouge-framer`](crates/rouge-framer). Install the Android SDK (`ANDROID_HOME`), a JDK (`java` and `keytool` on `PATH`), [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk), and the `aarch64-linux-android` Rust target, then run `cargo rouge setup android` once in `crates/rouge-framer/tests/fixtures/mobile` to install the pinned NDK, platform and build tools. `mise run run smoke android` builds, installs and starts the smoke app on a connected arm64 device or emulator and streams its log.

An Android app is a `cdylib` that exports `android_main` and gives the activity to Rouge:

```rust
use rouge::platform::AndroidApp;
use rouge::prelude::*;

#[unsafe(no_mangle)]
extern "Rust" fn android_main(android: AndroidApp) {
    let run = App::new()
        .android(android)
        .window(WindowOptions::default(), counter)
        .run();
    if let Err(error) = run {
        eprintln!("{error}");
    }
}
```

## Publish an app

Add the [`rouge-framer`](crates/rouge-framer) preset to your app's `.framer/` project and run `cargo framer`. It generates a GitHub Actions publishing workflow and a `cargo rouge` packaging command for the destinations you enable: GitHub Pages, Linux and macOS downloads, TestFlight/App Store and Google Play. Start with the workflow's `check` channel to validate builds without publishing or providing store credentials. Mobile `preview` uploads to TestFlight or Play internal testing. Mobile jobs do not run for `production`; promote the tested preview build in App Store Connect or Play Console. Downstream preset users can also deploy debug builds with `cargo rouge run ios` or `cargo rouge run android`. Those commands are separate from this repository’s `mise run run` example launchers.

See the [`rouge-framer` setup examples](crates/rouge-framer/README.md) for configuring a downstream app.

## Crates

For most apps, use only the `rouge` crate. It exports all of the other crates.

| Crate | What it does |
|---|---|
| `rouge` | The app, windows and platform hosts. The prelude. |
| `rouge-core` | Elements, styles, layout, text, events, the compositor and `TestUi`. |
| `rouge-widgets` | The widgets, fonts, icons and design tokens. |
| `rouge-reactive` | Signals, memos, effects, tasks and keyed lists. |
| `rouge-render` | The wgpu renderer and effects. |
| `rouge-macros` | `view!` and `#[component]`. |
| `rouge-theme` | Colors and palettes. |
| `rouge-markdown` | Markdown documents that you can parse in parts. |
| `rouge-syntax` | Tree-sitter syntax highlighting. |
| `rouge-framer` | An opt-in Framer preset for packaging and publishing Rouge apps. |

`rouge-reactive`, `rouge-theme`, `rouge-markdown` and `rouge-syntax` do not need a GPU or a window. You can use them in other apps, for example in a terminal UI.

## Limits

- Rouge does not support Windows.
- On Android, the soft keyboard edits a field through GameTextInput, which holds at most 64 KiB of text; a longer field takes committed text and key events only, without composition or autocorrect. Soft-keyboard composition on iOS has not been verified on hardware yet.
- Rouge draws all widgets itself. The widgets do not look like the native widgets of the platform.
- The macOS and Linux hosts expose widget roles, names and states through AccessKit. Plain-text fields expose their text, caret and selection, and accept selection and replacement requests. Native macOS range read/write has been verified; [VoiceOver listening checks](design.md#accessible-plain-text-editing) and Linux screen-reader verification are still pending. Android attaches `accesskit_android` to the activity's surface; TalkBack editing has not been verified on hardware. The browser has no accessibility bridge.
- Color emoji are not available yet.

For the full design and the list of planned work, read [design.md](design.md).

## License

Rouge is available under the [MIT license](LICENSE).

Bundled fonts and icons retain their own licenses, included alongside the assets.

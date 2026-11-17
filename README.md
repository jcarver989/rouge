# Rouge

Rouge is a cross-platform UI framework for Rust whose goal is to be "write once; run anywhere". It uses the GPU to render the UI via [wgpu](https://wgpu.rs) and currently supports MacOS, Linux and web browsers. 

Rouge is heavily inspired by [Leptos](https://leptos.dev/) and [GPUI](https://gpui.rs/). It uses signals for fine-grained state tracking, is GPU accelerated, and supports compositing, shaders, animations and effects.

> **Status:** Rouge is in early development. The API can change at any time.

## Why Rouge?

- **Fast updates.** A component function runs one time. When a signal changes, Rouge changes only the properties that read it. Rouge does not re-render and does not diff.
- **No work when idle.** When nothing changes, Rouge draws no frames.
- **Smooth animation.** Springs, tweens and keyframes run in the compositor. They do not cause layout or paint. Rouge has a target of 120 Hz.
- **Effects are built in.** You can use blur, backdrop blur, liquid glass, shadows, masks and blend modes. You can also write your own effects in WGSL.
- **One codebase for three platforms.** The same code runs on macOS, on Linux and in the browser (WebAssembly and WebGPU).
- **Two ways to write views.** Use Rust builder chains, or use JSX-style markup with `view!`. Both make the same calls, so you can use them together.
- **A full set of widgets.** Rouge has buttons, text inputs, menus, popovers, tabs, modals, sliders and more. Markdown and syntax highlighting are optional features.
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

Clone this repository, and then run these commands from its root.

| Command | What it shows |
|---|---|
| `cargo run -p rouge --example todomvc` | A full TodoMVC app with routes and saved data |
| `cargo run -p rouge --example component_gallery` | All of the widgets |
| `cargo run -p rouge --example demo counter` | The counter |
| `cargo run -p rouge --example demo glass` | Liquid glass |
| `cargo run -p rouge --example demo effects` | Blur, masks, blend modes and a custom WGSL effect |
| `cargo run -p rouge --example demo motion` | Springs, tweens and keyframes |
| `cargo run -p rouge --example surface` | GPU content from a different thread, shown with no copy |

### In the browser

You must have [`just`](https://github.com/casey/just) and `wasm-bindgen-cli` installed. The `wasm-bindgen-cli` version must be the same as the `wasm-bindgen` version in `Cargo.lock`.

```sh
just web                  # the component gallery
just web todomvc rouge    # TodoMVC
```

Then open <http://localhost:8000> in a browser that supports WebGPU.

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

`rouge-reactive`, `rouge-theme`, `rouge-markdown` and `rouge-syntax` do not need a GPU or a window. You can use them in other apps, for example in a terminal UI.

## Limits

- Rouge does not support Windows or mobile devices.
- Rouge draws all widgets itself. The widgets do not look like the native widgets of the platform.
- Accessibility is not available yet.
- In the browser, Rouge needs WebGPU. Full WebGL2 support is not available yet.
- Color emoji are not available yet.

For the full design and the list of planned work, read [design.md](design.md).

## License

Rouge is available under the [MIT license](LICENSE).

Bundled fonts and icons retain their own licenses, included alongside the assets.

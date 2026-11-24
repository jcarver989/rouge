# Rouge: High-Level Design

A signal-driven, GPU-composited UI framework for Rust, targeting macOS, Linux and the browser.

This document describes the design as the code implements it. Where a design is planned but not built, it says so; the [last section](#21-not-built-yet) lists everything not built yet. "Rouge" is a working name taken from the workspace directory.

---

## 1. Summary

Rouge is a general-purpose UI toolkit built on five ideas:

1. **Fine-grained signals.** Component functions run once. Every reactive prop is its own small effect that patches exactly one property of one retained element. There is no re-render, no diffing and no manual caching.
2. **A builder API, with JSX-style markup on top.** gpui-style chains (`div().flex().gap(8.0).child(...)`) laid out by taffy. The `view!` macro and `#[component]` attribute expand to the same builder calls, so both styles are first-class.
3. **One renderer on wgpu.** No per-OS drawing code. A custom instanced SDF renderer, written to fit WebGL2's limits, draws whatever the compositor hands it.
4. **A real compositor.** Layers, backdrop effects (liquid glass), custom WGSL effects, foreign GPU surfaces and animations that run inside the compositor without touching the reactive graph, layout or paint.
5. **One driver, a host per platform.** A winit host on desktop and iOS, a custom browser host and a custom Android host all drive the same frame loop.

The performance contract: a signal change costs work proportional to what changed, a compositor animation frame costs almost nothing on the CPU, and an idle app produces zero frames.

## 2. Goals and non-goals

### Goals

- Signals with automatic dependency tracking (the Solid / Svelte 5 model).
- A typed builder API with plain Rust functions as components, plus `view!` markup and `#[component]` for people coming from JSX. Both are supported; the builder is the underlying API.
- macOS, Linux (Wayland and X11) and the browser (wasm32 with WebGPU, and WebGL2's feature set) from one codebase, with all drawing through wgpu.
- Compositing and effects as first-class features: blur, backdrop blur, glass and refraction, shadows, masks, blend modes, user-supplied WGSL, zero-copy foreign GPU content.
- 120Hz: an 8.3ms frame budget, under 2ms CPU for compositor-only frames.
- Fully idle when nothing changes.
- A complete component set (`rouge-widgets`) built only on the public API.

### Non-goals

- Windows. Nothing in the design blocks it, but it is not targeted. iOS and Android run on the native host (section 14.2) and are experimental.
- Native-looking platform widgets. Rouge draws everything itself.
- A software renderer.
- System web views (WKWebView, WebKitGTK) as child windows. They cannot be composited: no glass over them, no overlays above them, no compositor transforms. Embedded web content is planned through surfaces and Servo (section 20).
- Any per-frame CPU copy of surface pixels, including as a fallback (section 12).

### A note on "avoiding Metal"

wgpu on macOS runs on Metal. What Rouge avoids is Metal-specific code: all shaders are WGSL and all GPU calls go through wgpu. The one exception is importing externally produced GPU memory (an IOSurface) as a wgpu texture for surfaces. That code lives in `rouge_render::interop`, the only module that allows `unsafe`; it creates and imports textures and never issues draw calls.

## 3. Decisions

### 3.1 At a glance

| Area | Decision |
|---|---|
| Scope | General-purpose toolkit for macOS, Linux and the browser |
| Reactivity | Fine-grained, run-once components, no diffing |
| Signal model | `Copy` handles into a runtime-owned arena; a single-threaded graph on the UI thread |
| API | Builder chains on functions returning `impl View`; `view!` and `#[component]` expand to the same calls |
| Tree updates | Binding effects push typed patches onto one queue; `Ui::step` applies them |
| Lengths | Logical pixels only; a per-window zoom multiplies the device scale |
| Layout | taffy (flexbox, grid, block) |
| Styling | Typed declarations; state variants are recorded style deltas; no cascade except inherited text properties |
| Theme | One `Signal<Palette>` over the renderer-neutral `rouge_theme::UiPalette`; each role is a memo |
| Text | Parley shaping and swash rasterization in `rouge_core::text`; a shaping cache keyed without colours |
| Compositor | A retained layer tree in `rouge-core`, owned by the element tree and called directly |
| Renderer | `rouge-render`: one instanced SDF pipeline, paged uniform records, no storage buffers (WebGL2-compatible) |
| Animation | Compositor-driven springs, tweens, keyframes and motion values; layout is never animated per frame by default |
| Effects | Built-ins plus the `ShaderEffect` trait for custom WGSL with up to 16 uniforms |
| Foreign content | Surfaces: a buffer queue from producers to a registry, shared zero-copy, never read back |
| Platform | One `Driver`; a winit host on desktop and iOS, a custom DOM host in the browser and a custom `android-activity` host on Android |
| Web view | Planned: Servo rendering into a shared buffer, in an optional crate |
| Performance | 120Hz on modern GPUs; idle is zero frames |

### 3.2 Protected decisions

These are deliberate. Do not remove or "simplify" them away.

| Decision | Why | Do not |
|---|---|---|
| **The browser (wasm32 + WebGPU/WebGL2) is a supported target.** | Rouge apps ship to the web from the same code. The renderer is written to WebGL2's limits so the browser can use either backend. | Treat the browser as optional, add features to the renderer that need storage buffers or compute, or let the browser host fall behind native. |
| **The browser host is custom** (`crates/rouge/src/platform/web/`), not winit's web backend, and shares `Driver` with native. | Text input needs a hidden textarea placed at the caret to receive IME composition, and clipboard needs the DOM's native `copy`, `cut` and `paste` events (which need no permission). winit's web backend provides neither. | Remove the browser host, or port it to winit. |
| **`view!` and `#[component]` are a first-class, supported API.** | Users coming from JSX expect markup. Both macros expand to builder calls, so there is still one underlying API and nothing is lost by mixing styles. | Remove the macros, or describe Rouge as macro-free. The builder remains the API the macros target. |
| **`rouge-theme`, `rouge-syntax`, `rouge-markdown` and `rouge-reactive` are standalone crates.** | `rouge-render` and `rouge-syntax` both need `rouge-theme`'s colours, and syntax must not depend on the GPU. Syntax and Markdown keep their heavy dependencies (tree-sitter grammars, pulldown-cmark) behind optional features. The reactive runtime is the layer below the UI. None depends on `rouge-core`, wgpu or any windowing crate. | Merge them into `rouge-core`, or give them UI or GPU dependencies. |

### 3.3 Rules that hold everywhere

- Components run once. A signal read in a component body (outside a closure) is read once and never tracked.
- Only `Ui` mutates the element tree: building views through a `BuildCx`, applying patches and handling input. Reactive effects, including binding effects and control-flow helpers, never touch the tree; they push patches.
- A dynamic prop computes its first value during build, so mounting an element costs one write and no follow-up patch.
- A change confined to compositor properties sets no layout or paint work.
- No surface pixel ever passes through CPU memory on the frame path.
- `unsafe` is denied workspace-wide except in `rouge_render::interop`.
- Each crate offers one way to do a thing: incremental and full parsing share one path, as do incremental and full highlighting (section 16).

## 4. Architecture

### 4.1 Crates

| Crate | Responsibility |
|---|---|
| `rouge` | Facade and prelude. `App`, `WindowOptions`, the shared `Driver`, and the platform hosts: `platform::native` (winit, for desktop and iOS), `platform::web` (custom browser host) and `platform::android` (custom `android-activity` host). Each target implements the `Platform` trait in its own module (`platform/{web,macos,ios,android,desktop}.rs`), and one `cfg_select!` in `platform.rs` picks the current one. Optional `persist` feature: `persisted` signals and keyed lists over an async `Storage` (`FileStorage` on native, `LocalStorage` in the browser). |
| `rouge-core` | Element tree (`src/tree/*`), styles, layout through taffy, events, focus, keymap, view builders, control flow through fragments, the compositor and animation engine, text (`rouge_core::text`, on Parley), theme, `Ui`, and `testing::TestUi`. |
| `rouge-render` | GPU only: `Gpu`, `Renderer`, draw lists, the glyph atlas, image cache, effects and their shaders, the surface registry and protocol, `headless`, `testing::TestSurface`, and `interop` (IOSurface allocation and import). |
| `rouge-reactive` | Signals, memos, effects, scopes, batching, context, tasks, resources, keyed lists, an optional Tokio bridge. No UI dependencies. |
| `rouge-macros` | `view!` and `#[component]`. |
| `rouge-widgets` | The component set: tokens, typography, bundled fonts, icons, materials, controls, menus, pickers (with fuzzy filtering through nucleo), panels; optional Markdown and syntax views. Built only on the public API. |
| `rouge-theme` | Renderer-neutral colours, `UiPalette` and `MarkdownPalette`, semantic control states. |
| `rouge-syntax` | Tree-sitter highlighting and symbol tags, incremental by appending. |
| `rouge-markdown` | Renderer-neutral Markdown documents and append streams. |

There is no `rouge-text`, `rouge-interop`, `rouge-web` or `rouge-platform` crate: text lives in `rouge-core`, interop in `rouge-render`, and both hosts in `rouge`. `rouge-devtools` and `rouge-webview` are planned and do not exist.

### 4.2 The frame

Every window owns one `Ui`: an element tree, the UI's replica of the layer model (`Layers`), a text system, a paint cache with the glyph atlas packer, and a patch queue. `Ui` is the platform-independent boundary that hosts and tests drive. A frame has two halves joined by a `Commit`: `Ui::update` runs the UI half, `Ui::commit` cuts the layer delta, draw lists, atlas uploads and surface ops since the last commit, and the host sends it to the compositor actor, which applies it and builds the scene (section 10.1).

**Writes.** Every element property is a plain-data `Op` (style, variant, text, content, cursor, state, condition, layer property, effect, scroll link or scroll offset). A static value is applied once when the element is built. A reactive value gets one effect: its first run applies the op inline while the element is built, so mounting costs one write and no follow-up patch, and later runs push `Patch::Element(id, op)` onto the window's queue, a single-threaded `Rc<RefCell<VecDeque>>`. Other patches carry images, canvas ops, edits, pastes, focus, motions or a `Build` closure for structural changes. Ops apply in the order they were written, so `.p(4.0)` then `.px(8.0)` keeps the horizontal 8 whether either is reactive. `Ui` applies patches in FIFO order. Applying a patch mutates the tree, and the tree writes layer changes to `Layers`, which applies each one to the UI's layer model and marks the layer as touched for the next commit.

**Style resolution** is deferred. A style or variant op writes the declaration into the element and marks it dirty, snapshotting its previous effective style once. At the end of each flush, and before layout and paint, every dirty element is resolved once, parents before children: its effective style and layout style are computed, its dirty level is compared against the snapshot, and its text subtree is refreshed only if an inherited text property changed. Ten changes to one element in one batch cost one resolution (`FrameStats::style_resolutions`).

**`Ui::step`** is one linear function, run by `Ui::update` (wall clock) or `Ui::update_at` (a test clock). Its phases are named in `crates/rouge-core/src/ui.rs`:

1. **Advance** the compositor's clock to the frame time, which settles finished curves, resolves finished motions and removes finished exits (section 11.1). Advance the caret blink phase; repaint the caret only when the phase flips.
2. **Mutate** (`flush`): poll ready tasks, then apply every queued patch inside one batch in the window's root scope, settle focus, apply a changed zoom; repeat until the queue is empty. Resolve dirty styles and publish the effective key bindings. Animations started here begin at the frame time.
3. **Animate**: publish compositor motion values and scroll-driven observers into their signals and flush.
4. **Settle layout**: lay out if anything is layout-dirty (taffy, then element bounds into layer-relative frames, then hover under the unchanged pointer); publish the geometry observers (`track_size`, `track_bounds`, `track_paint_region`, editor selections, scroll states) and flush; then at most one feedback re-layout (`FrameStats::relayouts`); then publish and flush once more, so paint-only feedback such as a canvas redrawing at its settled size lands in this frame. Anything that last flush makes layout-dirty waits for the next frame, and `next_frame` returns `Wake::Now` until it is laid out. Feedback is never an unbounded loop.
5. **Paint**: rebuild the draw lists of dirty layers and queue them for the next commit.
6. **Place surfaces**: compute each surface element's placement and queue changes for the registry; they travel in the next commit.

Every geometry observer is measurement feedback: virtual-list ranges and measured rows, popover and menu anchors, scroll-view thumbs and editor selections all decide what this frame paints, so moving any of them after paint would show a frame of lag. `VirtualListHandle` answers geometry questions on demand instead of publishing them.

Input methods (`pointer_moved`, `key_down`, `scroll`, `touch`, ...) flush before and after dispatching, and handlers run inside a batch, so a handler's writes land as one set of patches. Every flush first advances the compositor to `Ui::time`, so a handler's animations start at the event's time.

**`Driver::step`** (`crates/rouge/src/driver.rs`) is the host side of a frame, shared by both hosts: `ui.update()`, then `ui.commit()`, sent to the compositor actor as `Message::Commit` when it has content or the UI still wants a frame. The actor applies the commit, acquires the swapchain texture, latches surfaces, builds the scene, renders and presents, then runs `surfaces.presented(...)` and `surfaces.maintain()`. Its events come back into the `Ui` through `Driver::handle`.

### 4.3 What a change costs

| Change | Reactive work | Layout | Paint | Composite |
|---|---|---|---|---|
| Compositor animation frame (transform, opacity, a layer's background or radius, effect values) | none | no | no | yes |
| Scroll (wheel, drag, fling) | publishes the scroll state if observed | no, unless a virtual list's range changes | no | yes |
| A surface presents a frame | none | no | no | yes |
| Background colour of an element without a layer | one binding effect | no | its layer | yes |
| Text content | one binding effect | the leaf and its ancestors | its layer | yes |
| Theme switch | one memo per role; effects only for roles that changed | no | affected layers | yes |
| Nothing | none | no | no | no frame |

## 5. Reactivity (`rouge-reactive`)

### 5.1 Primitives

```rust
let count = signal(0);                          // Signal<i32>, Copy
let double = memo(move || count.get() * 2);     // Memo<i32> (a ReadSignal), lazy and cached
effect(move || println!("{}", double.get()));   // re-runs when double changes

count.set(1);
count.update(|c| *c += 1);
count.with(|c| *c > 0);       // borrow without cloning
count.get_untracked();        // read without subscribing
batch(|| { a.set(1); b.set(2); });  // one flush
```

- `Signal<T>` and `ReadSignal<T>` (`Memo<T>` is an alias) are `Copy` ids into a slotmap arena. `Signal` derefs to `ReadSignal`. `get` needs `T: Clone`; `with` does not. `set_if_changed` writes only when the value differs.
- Dependencies are collected dynamically on every run of a memo or effect.
- `signal.select(|state| part)` is a memo over one part of a larger value.
- `flag.toggle()` flips a `Signal<bool>`; `flag.pick(on, off)` is a closure that returns one of two values, for props that follow a boolean.
- `selector(|| current_key)` returns a `Copy` `Selector<K>`; `selector.is(key)` is a `ReadSignal<bool>`, and a change of the current key notifies only the old and the new key's readers, so marking the selected row of a long list is O(1) per change instead of one memo per row. A key's signal lives while any scope that asked for it does.
- `action(|input| async { … })` returns a `Copy` `Action<T, U>`: `dispatch(input)` starts the future, `pending()` is true until the latest dispatch resolves and `value()` holds its output. It replaces a `resource` keyed on a trigger flag.
- `stored(value)` returns a `Copy`, scope-owned `Stored<T>` that never subscribes readers, so handles that bundle state stay `Copy`.
- `keyed_list(|value| key)` returns a `Copy` `KeyedList<K, V>`: an ordered collection whose entries notify only their own readers. `prepend`, `push`, `extend`, `move_to`, `update(key, f)`, `update_all`, `remove` and `retain` write. `order()` publishes a shared `KeyOrder` snapshot with `get(index)` and `index_of(key)`. Its persistent sequence and origin-relative ranks let both end insertions avoid renumbering existing entries. A move updates ranks across the moved interval; removal reindexes the shorter side; bulk retain rebuilds ranks. An existing key passed to `prepend` or `push` updates its value in place. `keys()` still provides a `ReadSignal<Vec<K>>` for `each`, materialized only when read; `with(key, f)` and `select(key, f)` read one entry, and `with_values` reads them all in order and is notified by every write. Counts and filtered views are plain memos over `with_values`: one memo run and an O(n) scan per write. A `select` of a removed key keeps the last value it saw, because a row can recompute before `each` disposes it.
- `Callback<Args = ()>` is a `Copy`, scope-owned handler converted from `Fn()` or `Fn(T)` (as `Callback<(T,)>`). It defaults to a no-op, does nothing once its scope is disposed, and runs untracked with its creating scope as owner.
- `watch(source, |value, previous| ...)` runs a handler when a memo over `source` changes, starting immediately with `previous` as `None`.
- `interval(period)` returns a `ReadSignal<u64>` that ticks while its scope lives; `sleep(duration)` is a future.
- `Scope::current()` captures the owner, and `run_in(owner, f)` runs code later under it, skipping the call once it is disposed.

### 5.2 Propagation

Push-pull with graph colouring (the approach of Reactively and Leptos):

1. A write marks direct subscribers *dirty* and transitive subscribers *check*, and queues reachable effects.
2. On flush, each queued effect pulls its sources; a memo recomputes only if a source actually changed (`PartialEq`).
3. A rerun keeps the dependencies it reads again and unlinks only those it dropped. Subscribers and children are insertion-ordered sets, so unlinking is constant time.

This is glitch-free and runs each node at most once per flush.

### 5.3 Scheduling

- Inside a batch, writes do not run effects; they flush when the outermost batch ends.
- Outside a batch, a write flushes immediately. A write made while an effect or memo runs is deferred until it finishes.
- A flush that does not settle in 1,000 rounds fails a debug assertion naming the cause and stops.
- Effects always run before the next layout (section 4.2).

### 5.4 Ownership and disposal

- Every signal, memo, effect, task and scope is owned by the scope that was current when it was created. Scopes form a tree mirroring the UI: window root, component, each `show` branch, each `each` row.
- Disposing a scope frees its nodes, runs `on_cleanup` callbacks, cancels its tasks and removes its elements.
- Using a disposed handle panics in debug builds with its creation site (`#[track_caller]`). In release, writes are ignored and reads panic. `try_get`, `try_set` and `try_update` never panic.
- State that must outlive the place it was created is created in an ancestor scope and passed down, or provided through context.

### 5.5 Runtime, threading and async

- The graph is single-threaded and owned by an explicit `Runtime`. `runtime.enter()` returns a guard that makes it current on the thread; reactive calls panic when none is entered. Dropping the runtime disposes everything. The native `App` creates one runtime for all windows; in the browser each mounted canvas has its own; tests create and enter their own.
- `spawn(future)` runs a future on the runtime's UI-thread executor, so it can touch signals between awaits. It returns a `Task<T>` that can be awaited for `Result<T, TaskError>`. A task belongs to the scope that spawned it: dropping the handle does not cancel it unless `cancel_on_drop()` was called.
- `spawn_blocking(work)` (native only) runs synchronous work off the UI thread: on Tokio's blocking pool when a Tokio handle is set, otherwise on the runtime's elastic pool. Workers wait on one zero-capacity `crossbeam-channel`, so a job is handed to an idle worker when there is one and otherwise starts a `rouge-blocking` thread that runs it; a worker idle for 10 seconds exits. There is no thread cap. A panicking job reports `TaskError::Panicked` and leaves its worker alive. Dropping the `Runtime` lets idle workers exit; running jobs finish and their results are dropped, as with a disposed scope.
- With the `tokio-rt` feature, `set_tokio(handle)` (or `App::tokio`) enables `spawn_tokio(future)`. Cancelling the task or disposing its scope aborts the Tokio future.
- `resource(source, fetch)` gives an async-derived value with `get`, `loading` and `refetch`. It refetches when `source` changes; the previous fetch's task is disposed with the effect run that spawned it.
- `signal.sender()` returns a `Send` handle whose writes are posted to the UI thread.

### 5.6 Context

`provide_context(value)`, `use_context::<T>()` and `expect_context::<T>()` look up the scope tree. `Ui` provides the window handle, window size, zoom, theme and keymap at the root.

## 6. View API (`rouge-core`)

### 6.1 Example

The builder:

```rust
use rouge::prelude::*;

fn counter() -> impl View {
    let count = signal(0);

    div()
        .flex()
        .col()
        .gap(12.0)
        .p(16.0)
        .rounded(12.0)
        .backdrop(Glass::new().blur(24.0).refraction(0.4))
        .child(
            text(move || format!("Count: {}", count.get()))
                .text_size(18.0)
                .color(Role::Text),
        )
        .child(view! {
            <button label="Increment" test_id="increment" on_click={move || count.update(|c| *c += 1)}/>
        })
        .child(show(move || count.get() > 5, || text("That's a lot").color(Color::RED)))
}

fn main() -> Result<(), rouge::AppError> {
    App::new().window(WindowOptions::default(), counter).run()
}
```

The same view in markup:

```rust
#[component]
fn counter() -> impl View {
    let count = signal(0);
    view! {
        <div flex col gap=12.0 p=16.0 rounded=12.0 backdrop={Glass::new().blur(24.0).refraction(0.4)}>
            <text text_size=18.0 color=Role::Text>"Count: {count}"</text>
            <button label="Increment" test_id="increment" on_click={move || count.update(|c| *c += 1)}/>
            <show when={move || count.get() > 5}>
                <text color=Color::RED>"That's a lot"</text>
            </show>
        </div>
    }
}
```

### 6.2 Components and `view!`

- A component is a function returning `impl View`. It runs once, inside its own scope. Props are function arguments.
- `#[component]` makes a function usable as a `view!` tag. Each attribute naming a parameter becomes that argument; every other attribute is a method call on the returned value. A parameter named `children` receives the tag's children (lazily, as a closure, when its type is a closure).
- Parameters can be omitted when they have a fallback: `Option<T>` becomes `None` (a given value is wrapped in `Some`), and `#[prop(default)]` uses `Default::default()`. `#[prop(into)]` converts with `Into`; `#[prop(reactive)]` accepts a value, signal or closure for a `Prop<T>`.
- A bare attribute that names a parameter passes the same-named variable, or `true` for a defaulted `bool`; one that names no parameter calls a zero-argument method (`flex`, `col`).
- `name?=option` passes an `Option` to an optional parameter, or calls a method only when it is `Some`. `when:method=condition` and `when:method=(condition, argument)` call a method conditionally. `hover:bg=value` (any variant namespace) groups calls into `.hover(|s| s.bg(value))`. `let:name` names the argument a lazy child closure receives (`<each ... let:item>`).
- A string literal child with `{holes}` becomes a reactive `format!` closure. Inside `view!`, an `if` or `match` child erases its arms to `AnyView`; an `if` without `else` is an optional child.
- Builder structs that implement `View` remain the right shape for views that hand out a handle before they are built. `AnyView` erases types for heterogeneous children and recursion.
- **Widgets are constructors plus methods.** A widget's required value is its only parameter and every option is a method on the concrete type it returns, so `button("Save").disabled(saving).on_click(save)` and `<button label="Save" disabled=saving on_click=save/>` are the same call. Widgets record style calls into a `StyleDelta` and replay it with `StyleDelta::apply` after their own defaults, so caller styles win. `test_id` is an optional method, never a required parameter.

### 6.3 Reactive props

Every style and content method takes `impl IntoProp<T, M>`, implemented for a plain value (static: written once, no effect), `Signal<T>`, `ReadSignal<T>`/`Memo<T>`, closures `Fn() -> T`, and `Prop<T>`. The marker parameter `M` keeps the closure and value impls coherent and is invisible at call sites. Conversions go through `PropFrom` (so `f32` and `i32` become pixel lengths, `u32` hex becomes a colour, `&str` becomes `String`, and any `ShaderEffect` becomes an `AnyEffect`); a blanket `Into` bound made bare float literals ambiguous.

A reactive prop creates one binding effect. Its first run happens during build and applies its op to the new element inline; later runs push a patch.

### 6.4 Styling

- Typed tailwind-style methods on the `Styled` trait: `.flex()`, `.grid()`, `.w(200.0)`, `.w_full()`, `.px(4.0)`, `.gap(8.0)`, `.rounded(8.0)`, `.bg(color)`, `.border(1.0)`, `.border_color(color)`, `.shadow(BoxShadow::md())`, `.overflow_hidden()`, `.invisible()`. Every method records a `Declaration` through `Styled::declare(Prop<Declaration>)`. Layout declarations go to taffy. A grid container takes `.grid_cols(count)` for that many equal columns, and a child takes `.col_span(count)` and, to leave columns before it empty, `.col_start(column)` (columns count from 1; 0 goes back to automatic placement); rows are implicit.
- **Lengths are logical pixels.** Bare numbers and `px(value)` mean the same thing; `pct(value)` is a percentage of the parent; `auto()` lets layout decide. Text sizes and radii are pixel numbers; line height is a multiple of the font size. There is no rem and no design unit.
- **Zoom.** `zoom() -> Signal<f32>` (default 1) is a per-window zoom like browser zoom. `Ui::scale()` is the device scale times the zoom; the logical window (`window_size()`) is the platform size divided by the zoom; painting happens at the combined scale so text and paths stay crisp. Layout, hit testing and pointer positions are in the zoomed logical space; hosts divide physical pointer positions by `Ui::scale()` and multiply the IME caret area by it. `rouge_widgets::InterfaceSize` drives the zoom from a user-facing size.
- **Presentation.** `presentation() -> ReadSignal<Presentation>` says how the window is used: `input` is `InputKind::Pointer` or `Touch`, and `width` is `WidthClass::Compact` below `COMPACT_WIDTH` (600 logical points) and `Regular` otherwise. The host gives the input kind when it builds the `Ui` (`Ui::with_input`, changed later with `Ui::set_input`); the width class follows the logical window size. `Presentation::is_handheld()` is touch and compact together. Components read it; applications never branch on platform.
- **Keyboard and window scope.** `keyboard_inset() -> ReadSignal<f32>` is the height the soft keyboard covers at the bottom of the window, in zoomed points, fed by `Ui::set_keyboard_inset` and zero when there is none. `window_scope()` is the scope that lives as long as the window; a widget runs in it to provide context that views with no common ancestor share, which is how one registry of bottom chrome serves a tab bar, a toast stack and a floating button wherever each is mounted.
- **State variants are recorded deltas.** `.hover(|s| s.bg(Role::SurfaceHover))`, `.active`, `.selected`, `.checked`, `.focus`, `.focus_visible`, `.disabled`, `Div::group_hover(name, ...)` and `.when(condition, ...)` (any reactive boolean) run their closure once at build against a `StyleDelta` recorder and store a sparse list of declarations, applied over the base style while the state holds, in that order, so a later state wins. `.is_selected(prop)` and `.is_checked(prop)` drive the selected and checked states; a disabled element keeps them but drops hover, active and focus styles. Reactive values inside a variant stay live. An element keeps a boxed copy of its base taffy style only if a variant changes layout. `.group(name)` names the ancestor whose hover a `group_hover` follows.
- **Inherited text properties** are the only cascade: `text_color`, `text_size`, `font`, `font_weight`, `italic` and `line_height` can be set on any element, and each text leaf resolves each field from the nearest declaration, then `Role::Text` or the defaults. A colour change repaints inheriting leaves without layout; a typography change clears only affected leaf layouts.
- **Theme** is one `Signal<Palette>` provided through context (`Palette = UiPalette<Color>`); a subtree may provide its own. Colours in styles are a `Fill`: a `Color` or a `Role` (`bg(Role::Surface)`, `text_color(Role::TextMuted)`, `border_color(Role::Border)`), and any colour, hex value or role converts into one. Roles are resolved at paint against the palette of the element's theme, so a theme switch runs no effects: the next frame compares each theme's palette with the last one it painted, re-syncs layer backings and repaints. `Role` and the role-valued palette `UiPalette::ROLES` are generated from the one role table in `rouge-theme`; `Theme::control_style`, `selection_style` and `notice_style` return `SemanticStyle<Fill>` by resolving the component rules over that table. Code that needs a role's colour value rather than a style reads `theme.color(role)`, a memo, or `theme.resolve(fill)`. `Ui` provides `UiPalette::light()` at the root. Metrics are not theme roles: they are `rouge_widgets::tokens` constants and typography presets.
- `Typography` is a preset of font, size, line height and weight applied with `.typography(preset)`. Application shorthands are a plain extension trait over `Styled` with a blanket impl (as `rouge_widgets::UiStyles` provides `.body()`, `.caption()`), usable as bare attributes in `view!`.
- `.on_activate(f)` is the one interaction primitive: pointer cursor, activated by click, Enter or Space, and a tab stop by default. `.focus_policy(FocusPolicy::Roving)` takes it out of the tab order for container-managed focus and `FocusPolicy::None` makes it pointer-only. `.enabled(prop)` rejects pointer and keyboard activation, shows the default cursor and applies the disabled variant, all reactively. `.track_interaction(signal)` reports hover, press and focus as one `Interactions` value for the rare state variants cannot express.
- A view type that wraps an element implements `DelegateStyle` (`style_target`, returning the wrapped `Styled` value) and gets `Styled` from a blanket impl that forwards every declaration and state style to it.

### 6.5 Control flow and fragments

Components run once, so structure changes go through helpers that each manage child scopes:

- `show(when, || view)` mounts and disposes a branch; `.otherwise(|| view)` adds an else branch.
- `each(items, |item| key, |item| view)` is a keyed list. `items` is a closure returning a `Vec`, or a `Signal`/`ReadSignal` of any `AsRef<[T]>` collection, which is read in place at build time so a large list is never copied to be reconciled. Reconciliation is by key over the data: rows are moved, created or disposed, and surviving rows are untouched; an item is cloned only when its row is created or its value changed. Each row receives its item as a `ReadSignal` so in-place updates stay fine-grained.
- `switch(on, |value| view)` remounts when the memoized value changes; the view is built untracked.
- `dynamic(|| view)` re-runs a closure and replaces its subtree. Reusing the same `FocusHandle`, `VirtualListHandle` or scroll-state signal across replacements restores focus, caret, selection, history and scroll offset.
- `overlay(|| view)` portals content to the window's overlay root.

`View::build` builds into the current parent rather than returning an element. Each helper owns a **fragment**: a contiguous run of children in its real parent, recorded in a side table, with no element or layout node of its own. Fragments nest (each `each` row is one), keep their place among siblings as they grow and shrink, and are what removal, reordering and enter/exit transitions act on. `()`, `Option<impl View>` and `Vec<impl View>` build zero or more children in place, so an absent child needs no placeholder. `Apply::when` and `Apply::when_some` do the same for builder chains. Plain `if` and `for` work for structure that is static after construction.

### 6.6 Element set

`div`, `text`, `rich_text`, `image`, `icon` (a filled or stroked `Path`, including `Path::svg` path data), `canvas` (a draw callback with a `Painter`), `editable`, `scroll`, `virtual_list`, `surface` (section 12) and `overlay`.

## 7. Element tree and layout

### 7.1 Elements

- Elements live in a slotmap arena. Each holds its style (plus a boxed resolved style while a variant applies), a content `Kind` (`Div`, `Text`, `Image`, `Canvas` or `Surface`, with any payload boxed), up to two layers (its own, and a scroll container's content layer), its children, bounds, interaction states and flags. Everything rarely used (listeners, actions, key bindings, focus, scroll, variants, groups, observers, test ids, text hit callbacks) lives in one optional boxed `Extras`.
- Each element's layout node (its resolved taffy style, taffy's layout cache and its unrounded placement) lives in a `SecondaryMap` keyed by the element's id, beside the arena rather than inside each element: inline, it would grow every element from about 300 bytes to about 1.5 KB and slow every pass that walks elements without laying them out.
- The tree changes only when `Ui` applies patches. Nothing is rebuilt per frame.

### 7.2 Dirty levels

The tree keeps one dirty level, `None < Composite < Paint < Layout`, plus a dirty flag per layer. A patch sets the narrowest level:

| Level | Caused by | Consequence |
|---|---|---|
| `Layout` | sizes, spacing, flex and grid properties, position, text content and typography, structure | taffy relayout, then repaint of layers whose content moved |
| `Paint` | background, border colour, radius and shadow of an element without a layer, text colour, canvas output | rebuild the draw lists of the owning layer only |
| `Composite` | layer transform and opacity, a layer-backed element's background and radii, effect values, scroll offsets, motion values | update compositor state; no layout, no paint |

### 7.3 Layout

- taffy drives flexbox, grid and block layout. Text, images and measuring canvases are measured leaves.
- There is no separate taffy tree. A borrowed view over the element arena and its layout nodes implements taffy's low-level traits (`TraversePartialTree`, `LayoutPartialTree`, `CacheTree`, `LayoutContainingBlock`, `LayoutBlockContainer`, `LayoutFlexboxContainer`, `LayoutGridContainer`), so taffy reads children straight from the elements and styles from their layout nodes, and writes its cache and unrounded placements back into those nodes. Taffy node ids are the element keys.
- taffy's cache confines relayout to affected subtrees. A layout-affecting change (a changed layout style, text, image size, canvas height, or a child inserted or removed) clears the cache of the element and of each ancestor up to the first one already cleared. The root is laid out with `compute_root_layout`. There is no separate rounding pass: the bounds pass that already walks every element snaps each placement to whole pixels as taffy's `round_layout` would, measuring an absolutely positioned element from its containing block (the nearest positioned ancestor, or the root), so taffy's list of each containing block's positioned descendants is never stored.
- Bounds are resolved into frames relative to the owning layer, so moving a layer never invalidates its contents. Transforms never affect layout.
- Layer origins and scroll offsets snap to device pixels so text stays sharp; animated translations do not snap, except on a rasterized layer, which composites its cached texture at a whole device-pixel offset.

### 7.4 Scrolling and virtual lists

- `scroll()` returns a `Div` whose children live in a content layer; its offset is a compositor property. Scrolling changes no element, runs no binding effect and repaints nothing. It scrolls both axes and has no scrollbars (`rouge_widgets::scroll_view` adds them).
- `.scroll_state(signal)` attaches a `Signal<ScrollState>` (offset, viewport, content) that is published after layout and accepts programmatic offsets. A scroller at its limit passes the wheel to its nearest scrollable ancestor.
- `Div::scroll_linked(state, ScrollLinked::Sticky { lo, hi } | ScrollLinked::Linear { .. })` moves a layer in the compositor relative to a scroll container, so sticky headers and scrollbar thumbs cost no reactive work per frame.
- `virtual_list(keyed_list, row_height, row)` takes identity and order directly from the model. An entry keeps its identity through moves and value updates; removing and reinserting a key creates a new entry, even within a batch. A prepend looks up the old anchor and mounted keys without enumerating the list. If the anchor is removed, the list searches the previous snapshot for a surviving successor, then a predecessor. `virtual_list(count_fn, row_height, row)` remains available; an explicit indexed `.row_identity(...)` rebuilds a full `KeyOrder` snapshot on invalidation, so both forms share one code path. Either form mounts only rows in the visible range plus overscan, derived from the offset. Heights are a uniform estimate (constant time until a row is measured) or per index (a prefix index updated in logarithmic time). `.measure_rows(revision)` lays rows out at natural height and caches measurements. With a uniform estimate, only measured deviations need prefix-sum storage; order changes remap cached measurements, so cost can grow with the number of rows measured. Per-index estimates still require full initialization on structural changes. `.row_identity(|i| RowKey::new(id, revision))` retains mounted rows by ID and revision, not by position, and keeps the top visible item stable across data changes. The row builder receives its mount-time index; for position-dependent bindings or event handlers, capture `virtual_row_index()` in the row builder and read that memo when needed. A new revision remounts the row; `.follow_end(signal)` sticks to the end; `.track_rows(signal)` reports the visible range. `VirtualListHandle` offers `scroll_to_row(index, Align)`, `scroll_to`, `top_row`, `row_top` and `row_bounds`, all computed on demand rather than published per frame. A `ScrollState` write is either a move or a re-anchor: `scroll_to(offset)` goes there and stops any momentum, while `anchor_to(offset)` says the content moved under the viewport (a row above it was measured at a new height), so a fling in flight is shifted by the difference and keeps its velocity. `.underlay(view)` places one view inside the scrolling content behind the rows, for a highlight that moves between rows and scrolls with them.
- Wheel deltas scroll and clamp. Trackpad gestures with phases go through `Ui::scroll_drag`, which follows past the limits with resistance, and `Ui::scroll_release`, which flings from the measured velocity with decay and bounces off the limits; the native host swallows the OS's own momentum deltas after a lift. Touch pans use the same path.

## 8. Text (`rouge_core::text`)

- **Parley** shapes and lays out: bidi, line breaking with a break-word fallback for overlong tokens, font fallback, system font discovery through fontique on native, rich spans, tab stops and a hanging `WrapIndent`. The browser build has no system fonts; apps add font bytes (`App::fonts`, `rouge_widgets::bundled_fonts`).
- **swash** rasterizes glyphs into the alpha atlas with four horizontal subpixel buckets and grayscale antialiasing.
- **One shaping cache per window.** `TextSystem::layout(text, style, max_width)` returns a `TextLayout` holding an `Rc` to an already broken and aligned layout. The key holds only what affects shaping: content, font properties, alignment, wrapping, tab size, span boundaries and the font fields of spans, never colours. Span colours and decorations are a palette resolved at paint. Each entry keeps up to four line-broken widths; two generations of 4,096 entries replace a full clear. Adding a font clears the cache. `TextStats` counts shapes, line-breaking passes and hits.
- `text` and `rich_text(content, spans)` are measured leaves. They wrap unless told `.no_wrap()`, which overflows, or `.truncate()`, which stays on its line and ends in an ellipsis where room runs out. Truncated text can shrink to its ellipsis, so it needs no `min_w` or clip around it, and it keeps all of its content for selection, search and accessibility. Overlapping spans compose per field; `on_range_hover` and `on_range_click` report the byte under the pointer.
- **Editing** is the core `editable(signal)` element over Parley's cursor and selection: movement by character, word, line and document, IME composition, clipboard, grouped undo and redo, a bidirectional selection signal in UTF-8 byte offsets, min and max rows with caret scrolling, placeholder, submit on Enter, and a caret that blinks for ten seconds after activity. `rouge_widgets::text_input` wraps it (`multiline` makes it a text area). `rouge_widgets::inline_edit` edits a value in place: Enter or losing focus (`Editable::on_blur`) commits once, and Escape cancels.
- **Canvas text.** `Painter` draws `TextRun`s (clipping, wrapping, continuation marks, right alignment, hit tags, names) through the same cache. `Canvas::measure` makes a canvas a measured leaf sized from its text in the same layout pass, which is the supported route for dense content such as diffs and code.

### Accessible plain-text editing

`editable` and widgets built on it expose text runs while the desktop AccessKit adapter is active. Focus stays on the labeled input. Runs follow logical reading order; their bounds, character advances and direction come from the same Parley layout that draws the text. AccessKit maps run-relative character positions to platform indices (UTF-16 on macOS). Setting a selection preserves its anchor and focus and goes through the editor that handles keyboard input, replacement and undo. The placeholder is not part of the value.

Run IDs occupy a namespace separate from element and content-layer IDs (bit 32 is clear). A run keeps its ID across selection and geometry changes; a changed run value or character breakdown gets a new ID. With no scroll or reflow, a caret move publishes only the field, and a blink publishes nothing. AccessKit stores a selectable character's UTF-8 length in one byte: if a layout has a cluster longer than 255 bytes, the field exposes its value without text ranges rather than publishing incorrect offsets.

**macOS release check.** Native `AXUIElement` calls on macOS 26.2 found `AXTextField` **Display name** and `AXTextArea` **Notes** in the settings example. Writing `AXSelectedTextRange` and reading `AXSelectedText` returned `café 😀` for UTF-16 range `{4,7}` and `word \ncafé 😀` for `{109,13}` across a newline. This checks the OS API, not what VoiceOver says. To complete the listening check:

1. Run `mise run run settings`. With VoiceOver, find **Display name** and **Notes** by name. Confirm each is announced as an editable field and its value is read once, not as separate text runs.
2. In each field enter text including `café 😀`. In **Notes**, add enough words to wrap and insert a newline. Move by character and by word, including over the emoji, wrap and newline; compare speech with the visible caret.
3. Extend a selection, shrink it back through its anchor and extend in the opposite direction. Compare the spoken selection with the highlight. Type a replacement, then press Command-Z once. Verify the original text and selection return and focus stays in the field. Record the macOS/VoiceOver version and any discrepancy before signing off.

To compare the cost with and without accessibility, run `cargo run --locked --release -p rouge-bench -- run editor_caret editor_typing --runs 3`. Both variants drive the same 2,000-line editor for 200 measured frames. On a local arm64 Mac, median headless CPU time per frame was 4.32 / 4.41 ms (off / on) for caret movement and 19.93 / 22.38 ms for typing. Enabling accessibility added no shapes in either workload. These are not screen-reader or display frame times.

## 9. Rendering (`rouge-render`)

### 9.1 Primitives

One instanced pipeline (`primitive.wgsl`) draws every primitive with analytic coverage in the fragment shader, in the style of gpui:

| Primitive | Notes |
|---|---|
| Quad | Per-corner radii, per-edge borders, solid fill |
| Shadow | Analytic blurred rounded rectangle, no offscreen pass |
| Glyph | Alpha mask from the atlas, tinted |
| Image | RGBA or alpha-mask texture, source rect, tint, rounded clip |
| Path | Rasterized on the CPU by zeno into an alpha mask at device scale, cached per path, stroke, scale and subpixel offset, and drawn as an image |

Every instance carries an axis-aligned clip rectangle. Underlines and strikethroughs are quads. A layer-backed element's background and radii are not in its instances: a "backing" quad reads them from the layer's per-draw record, so animating them uploads nothing.

### 9.2 Buffers

- **Draw lists.** Painting a layer produces one `DrawList` per paint-order segment (segments separate nested layers). A list has an id and a revision.
- **One shared instance buffer.** Lists are uploaded into power-of-two slots keyed by list id. An unchanged revision uploads nothing. Slots idle for 600 frames are dropped and the buffer is compacted when less than half of it is live.
- **Paged uniform records.** Per-draw data (layer transform, opacity, placement, fill, radii, and an effect's mapping and parameters) is appended each frame to 16 KiB pages and bound as a uniform buffer with dynamic offsets. 16 KiB is WebGL2's maximum uniform block, and the renderer uses no storage buffers and no compute, so it fits WebGL2.
- Hash maps use FxHash; compositor layers live in a `SlotMap` keyed by `LayerId`, and a scroll source is keyed by its state signal's id, so two scroll states never share a key. Effect pipelines are looked up by the address of their `&'static` WGSL source, one pipeline per distinct shader, without hashing the source text.

### 9.3 Atlas, images and caches

- One alpha-mask glyph atlas per window, 2048 pixels wide, shelf-packed, growing to 8192 rows; only dirty rows are uploaded.
- Images upload once per `ImageData` id and are evicted after 600 idle frames or, least recently used first, above a 256 MiB budget. Draws batch per run of same-texture primitives. `image(source)` decodes off the UI thread on native (`spawn_blocking`) and on the main thread in the browser.
- Rasterized layers and backdrops have cached textures, described in section 10.

### 9.4 Colour and blending

- `Color` is RGBA in sRGB-encoded floats (`Color::rgb(0xRRGGBB)`, `from_rgba8`, `hsl`, `with_alpha`).
- The swapchain uses a non-sRGB format and compositing blends sRGB-encoded values, matching CSS and design tools. Surface buffers follow the same rule (section 12).

### 9.5 Frame pacing

- `Ui::next_frame()` returns a `Wake`: `Now` for queued patches, dirty state, or a compositor animation that something on the UI side follows: a moving layer under a window-geometry observer (`track_bounds`, `track_paint_region`, tracked text), or a scroll observer's own flinging layer. `track_size` and editor selections never follow a compositor animation. Whether anything is followed is decided once per update, so asking costs nothing per call; `At(time)` for the next flip of a blinking caret (on the `Ui::time` clock) or the end of a motion an `Animation` is waiting on; otherwise `Idle`. A compositor animation nobody observes does not wake the UI: the compositor draws it on its own.
- **The frame token.** The UI runs layout and paint only while it holds its window's token. After a step, `Driver::step` sends the commit if it has content or the UI still wants frames (an empty commit is a frame request); that gives the token to the compositor, and the `Presented` that acknowledges it gives it back. This keeps the UI at most one frame ahead and coalesces input between frames. A frame is drawn only when it is due or a visible surface has or wants a frame. The native host turns `At` into `ControlFlow::WaitUntil`; the browser host into a `setTimeout` that then requests an animation frame. A focused, idle editor therefore draws two frames a second, not one per display refresh, and none after its caret stops blinking.
- `Driver::resize` clamps the surface to the device's maximum texture size for every host. The native host caches each window's refresh interval, refreshing it when the window moves or changes scale, and handles window commands only for the window an event reached.
- Presentation is `AutoVsync` (overridable with `ROUGE_PRESENT_MODE`, or `?rouge_present_mode=` in the browser). The animation clock is the time at `Ui::update`, and animations are sampled at it (section 11.1); surfaces latch their newest frame just before the window draws.
- Surface producers do not run timers: they ask for frames and the compositor answers after each present (section 12).

## 10. Compositor, layers and effects

### 10.1 Layers

The layer model (`rouge_core::LayerModel`) is plain data: layers with their hierarchy, frames, properties, transitions, keyframes, effects, scroll state and exits, plus the motions. It holds no draw lists, GPU types or `Rc`, and every animated value in it is a pure function of time (section 11.1). Two copies exist:

- **`Layers`** (UI side) is the only writer. The element tree calls it as patches apply and as layout and paint run; each call mutates the UI's copy and marks the layer or motion as touched. Reads (`pose`, `scroll`, `motion`, `has_surfaces`) are answered synchronously from this copy, so hit testing, geometry and scroll targets never wait for the compositor. `Completion`s stay here: an advance past a motion's end resolves its `Animation` with `true`, and a write that interrupts it resolves `false`, so the model holds no `Rc`. `Layers::commit` cuts a `LayerDelta`: the current state of every layer and motion touched since the last commit (or its removal), the root, the clock, and the time the last running animation settles.
- **`Compositor`** (scene side) owns the replica, the `DrawLists` and the command list. Applying a commit stores each delta entry under the UI's `LayerId`, frees the lists of removed layers, stores the commit's draw lists, and binds and unbinds surfaces. The replica is never advanced or written otherwise; it is only sampled at the present time, so it cannot diverge from the UI's copy. It skips a layer whose exit has finished until the UI's next commit removes it.

A `LayerDelta` is `Clone` and `Debug`, and `LayerModel::apply` accepts one on any model, so a recorded sequence of deltas replays into a fresh model and can be sampled at any time (`tests/compositor/layers_test.rs`). That is the hook for devtools record and replay. The glyph atlas stays whole on the UI side: `Atlas::take_uploads` returns the rows dirtied since the last commit (every used row after the atlas grows), and `Renderer::upload_atlas` writes them to the atlas texture.

Each frame, `Compositor::scene(scale, &surfaces, at)` refills one flat command list the compositor owns, cleared without freeing its storage, and hands the renderer a `Scene`: that list plus the compositor's `DrawLists`, the arena holding every layer's retained draw lists. Commands are `Command::Draw` (draw a list by `ListKey` with a transform, opacity, fill, radii, scissor and optional backdrop effect), `Command::Group` (a header for the next `len` commands, which render into a cached texture that is then drawn, optionally through an effect) and `Command::Surface`. A group header carries its signature and bounds, computed by `Scene::summarize` over its children when the group closes; a surface command carries the number of the frame latched for it, so a new surface frame changes the signature of every group around it. The renderer plans straight from the list and skips a fresh group's children by `len`, so neither side builds a second tree or walks the scene twice.

A layer is a retained set of draw lists plus compositor properties: translate, uniform scale, rotate, opacity, fill colour, corner radii, scroll offset, scroll link, effects and a surface binding. An element gets its own layer when it declares any of them: `.layer()`, `.rasterize()`, `translate_x/y`, `scale`, `rotate`, `opacity`, `.transition`, `.layout_transition`, `.animate`, an effect, backdrop, mask or blend, `.scroll_linked`, or a surface. Elements under `show` or `each` with enter or exit transitions are promoted when they mount, so there is no hitch on the first animated frame. Everything else paints into its nearest ancestor layer.

### 10.2 Direct and rasterized layers

- **Direct** (the default): the layer's retained lists are drawn into the parent target with its transform and opacity in the per-draw record. SDF primitives stay crisp under scale and cost no texture memory.
- **Rasterized**: the layer renders into a cached group texture, then draws as one quad. This happens when it has content effects, a mask or a blend mode (each a stage, applied in that order), when it is translucent and holds more than one primitive, or when `.rasterize()` is set. A translucent layer with a backdrop reader somewhere below it is not rasterized and falls back to per-primitive opacity. A group's contents are rasterized in group-local space, with the layer's translation removed, so moving or fading a rasterized layer reuses its texture. A group texture is re-rendered only when the fingerprint of its contents changes, including the frames of any surfaces inside it, or when the part of it inside the viewport changes. Group textures, like images and instance slots, are dropped after 600 frames unused, so an inner stage whose outer stage is cached, or a layer briefly hidden, keeps its texture.

A compositor-only frame therefore updates records and re-issues draws for existing buffers.

### 10.3 Backdrop effects

A backdrop effect reads what has already been drawn behind an element.

1. The scene renders into the persistent scene target (see Damage below), which is blitted to the swapchain at the end.
2. On reaching a layer with a backdrop effect, the region under it, grown by the blur's reach, is copied from the scene into that backdrop's retained clean copy, but only within this frame's damage, so pixels outside it keep what was behind the layer rather than the layer drawn over it last frame. The clean copy is blurred with a dual-Kawase down/up chain at reduced resolution into a backdrop texture.
3. The layer's backdrop shader draws, sampling that texture, followed by the layer's content.

Cost controls: consecutive backdrops share one pass break when nothing drawn since the break touches the later one's region (each still runs its own blur); a backdrop is reused with no pass break when everything drawn before it that reaches its region hashes the same as last frame, so an animation elsewhere on screen does not re-blur it; blur radii are continuous, though the kernel changes shape at each power of two. An element has at most one backdrop effect. Inside a rasterized group, backdrop effects are skipped and blend modes draw normally, because they only see the window's scene.

Damage: the window's scene lives in a persistent texture that is copied to the target each frame, and only what changed is redrawn into it. The renderer fingerprints each top-level command with its device extent and diffs the list against the last frame; the extents of commands that appeared, disappeared or changed are cleared to the clear color and redrawn, every step clipped to them, in one pass so each backdrop is still snapshotted once. When the damage touches the region a backdrop reads, the backdrop is re-blurred from its clean copy, and only the part of the layer within the effect's reach of the damage is redrawn: four coarsest blur texels plus how far the effect displaces its backdrop samples (`ShaderEffect::backdrop_reach`, for example glass refraction). An effect that does not declare a reach redraws its whole region. The whole frame is redrawn on the first frame, a resize, a change of scale or clear color, a reordering of unchanged commands, or a backdrop appearing or disappearing; damage is kept as at most six disjoint rectangles before collapsing to their bounds. Each layer's draws are clipped to its content's extent as well as its scissor, so a layer the damage does not touch is not encoded at all. `Renderer::draw` records the frame and only then asks its caller for a target, telling it whether anything changed; a frame with no damage acquires no drawable and presents nothing, and a presented frame is one command buffer and one submit. `RenderStats::damaged` counts the pixels redrawn and `RenderStats::presented` the frames shown.

### 10.4 Clipping

Within a layer, `overflow_hidden` clips through per-instance rectangles. Nested layers are clipped with scissor rectangles: exact for axis-aligned content, the bounding box under rotation, never rounded. `.mask(Mask::shape())` clips a layer to its rounded shape at the cost of rasterizing it.

### 10.5 Effects API

Built-ins are style methods taking reactive props:

```rust
div().effect(Blur::new(8.0))                       // content effects, applied in order
     .effect(ColorMatrix::saturate(1.4))
     .backdrop(Glass::new().blur(24.0).refraction(0.4).dispersion(0.05).tint(tint))
     .mask(Mask::shape())
     .blend(BlendMode::Screen)
```

Built-ins: `Blur`, `BackdropBlur`, `ColorMatrix` (3x4: RGB with offset, alpha untouched), `Mask` (linear and radial gradients, shape, inverted), `BlendMode` (twelve modes, reading content and backdrop, so a blended element costs a backdrop snapshot) and `Glass`. `Glass` is a backdrop shader that derives an edge normal and thickness from the element's rounded-rect SDF, offsets backdrop UVs for refraction, splits channels for dispersion, then adds tint, saturation, a configurable rim light (`rim_light(angle, fill)`), an optional point light and an inner shadow. `Glass::frosted(blur)` is plain frosted glass.

Custom effects implement `ShaderEffect`:

```rust
struct Ripple { frequency: f32, strength: f32 }

impl ShaderEffect for Ripple {
    const WGSL: &'static str = include_str!("ripple.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;           // or BACKDROP, or BOTH

    fn uniforms(&self, uniforms: &mut [f32; 16]) {
        uniforms[..2].copy_from_slice(&[self.frequency, self.strength]);
    }

    fn margin(&self) -> f32 { self.strength.abs() }   // extra area drawn and sampled
}
```

```wgsl
fn effect(in: EffectInput) -> vec4<f32> {
    let away = in.local - in.size * 0.5;
    let distance = max(length(away), 1e-4);
    let wave = sin(distance * param(0).x) * param(0).y;
    return sample_content(in.local + away / distance * wave);
}
```

The WGSL defines `fn effect(in: EffectInput) -> vec4<f32>` returning premultiplied colour. A prelude provides `in.local` (logical pixels from the element's top-left), `in.size`, `in.uv`, `in.distance` (signed distance to the rounded rect) and `in.coverage`; `sample_content(local)` and `sample_backdrop(local)`, which follow the layer's transform; and `shape_sdf(local)`, `scale_factor()` and `param(i)`, the i-th `vec4` of the 16 uniforms. `content_blur()` and `backdrop_blur()` ask for an input to be blurred first (that is how `Blur`, `BackdropBlur` and `Glass` work). `const CLOCK: Option<usize>` names a uniform that receives elapsed seconds, which keeps the layer animating while the effect is present.

Effect values accept reactive props (`.effect(move || Blur::new(radius.get()))`). When the bound value changes on an element with a `.transition`, every uniform, the margin and the blur radii animate in the compositor; a different shader replaces the effect.

## 11. Animation

### 11.1 Compositor-driven

Animatable without layout or paint: translate, uniform scale, rotate, opacity, scroll offset, the background colour and corner radii of a layer-backed element, and effect values. Clip is not animatable.

```rust
// Transitions: animate whenever a bound value changes.
div().opacity(move || if open.get() { 1.0 } else { 0.0 })
     .scale(move || if open.get() { 1.0 } else { 0.95 })
     .transition(Spring::snappy())

// Motion values: imperative and compositor-owned.
let x = motion(0.0);
div().translate_x(x)
     .on_click(move |_| { x.animate_to(200.0, Spring::bouncy()); })

// Keyframes: declarative, one-shot or repeating.
div().animate(Keyframes::new().at(0.0, |p| p.rotate(0.0)).at(1.0, |p| p.rotate(360.0)).duration(0.8).repeat())
```

- With a transition, the binding effect runs once per change and hands the compositor a target; interpolation then happens entirely in the compositor. Springs keep velocity when retargeted.
- `Motion` is `f32`: `set`, `animate_to`, `decay`, `stop` and `get` (the value as of the last frame). `animate_to` and `decay` return an `Animation` that can be ignored or awaited for `true` on arrival or `false` when interrupted. Gestures can drive a motion directly: `fling_to(target, velocity, timing)` starts a spring from a release velocity, and `motion.map([from, to], [first, last])` binds a second property to the same value through a clamped linear mapping (a sheet's scrim opacity from its offset), still sampled in the compositor.
- Timing is `Spring` (`snappy`, `smooth`, `bouncy`, or custom), `Tween` (seconds plus `Ease`: linear, in, out, in-out or `cubic_bezier`) and, for motions and flings, `Decay`.
- Animation is sampled, not ticked. Each animated value is a curve with a start time, and its value and velocity at any time are pure functions of the elapsed time: springs use the closed-form solution of the damped spring (under-, critically and over-damped), tweens ease their progress, and decay is `x0 + v0 / rate * (1 - e^(-rate * t))`. A fling is a decay segment up to the time it crosses a scroll limit, solved in closed form, followed by the bounce spring from the crossing state; an extent change during the glide rebuilds it. Every curve computes its end time when it starts (for springs, the time after which value and velocity stay within the rest thresholds), so the compositor knows when the last animation settles without being told, and a fling knows where it will rest.
- The UI's layer model has a clock that only moves forward. `Layers::advance(time)` sets it, settles curves that have ended, resolves finished motions and removes layers whose exit has finished. Every other call takes effect at the clock's current time: an animation it starts begins then, and a retarget or a scroll samples the old curve then, so springs keep their velocity. Sampling at a time therefore does not depend on how time was divided between advances. `Ui::update` advances to the frame time and input dispatch advances to the event's time, so a gesture released after a long idle starts its fling at the release. After a stall, animations skip ahead to where they would be instead of slowing down.
- Keyframes take `Pose`s over the compositor properties, a duration in seconds and one easing per segment; one-shot keyframes hold their final pose. Repeating keyframes are phase-locked to the clock rather than to when they start: a loop's cycles begin at multiples of its duration, so loops of equal or commensurate durations change in the same frames instead of each waking the compositor on its own. Effect values cannot be keyframed.

### 11.2 Layout transitions

`.layout_transition(Spring::smooth())` gives FLIP animation: when layout moves an element relative to its parent layer, layout runs once to the final state and the compositor animates an offset from the old position. `LayoutTransition::new(timing).size()` also animates size by stretching the layer, which scales its content while the transition runs.

### 11.3 Enter and exit

`show` and `each` accept `.enter(pose, timing)` and `.exit(pose, timing)`, for example `.enter(Pose::new().opacity(0.0), Spring::snappy())`. Rows present at first mount enter too. On exit, the scope is disposed immediately, and the layer stays in the compositor as a non-interactive orphan, drawn above its parent's content, until the animation finishes.

### 11.4 Layout properties

Width, height, padding and gap are not compositor properties. Animating them means writing a signal each frame, which relayouts the affected subtree per frame. That is correct and bounded, but it is the slow path; prefer layout transitions.

## 12. Surfaces

A **surface** is a layer whose content is a GPU texture produced outside Rouge's painter: a 3D viewport, a video decoder, a camera feed, a web view. It is the single way foreign pixels enter the scene, and no consumer is special-cased in the compositor.

**The rule.** The frame path never moves surface pixels through CPU memory. Rouge never reads a surface back and never writes surface pixels with `queue.write_texture`, and there is no fallback that does either. A producer that cannot share GPU memory with the compositor's device gets a typed `SurfaceError` and the element shows its fallback view. A 1600x1200 surface at 2x is 30.7 MB per frame; copying it out and back at 120Hz is 7.4 GB/s of memory traffic for no visual result. A producer that renders on the CPU (a software decoder) writes into a shared buffer itself; that is its rendering, not a copy made by Rouge.

**Because a surface is an ordinary layer**, everything in sections 10 and 11 applies: transforms, opacity, rounded clipping, scroll containers, enter and exit, layout transitions, glass over it, overlays above it, hit testing through animated transforms.

### 12.1 Shape

Surfaces are a buffer queue between a producer and a registry, the shape every OS compositor converged on (Wayland's `linux-dmabuf` with frame callbacks and explicit sync, Core Animation's IOSurface-backed contents, Android's `BufferQueue`, Chromium's viz surfaces):

| Party | Type | Thread | Role |
|---|---|---|---|
| Producer | `SurfaceProducer` | its own (an actor) | Owns its rendering; configures its buffer pool, acquires and submits buffers, reacts to events |
| Registry | `Surfaces` | UI thread | Holds imported buffer sets, frame queues, each surface's current frame, release bookkeeping and frame requests |
| Element | `surface(source)` | UI thread | A layout leaf that owns a layer bound to the surface's `SurfaceId`, and reports its placement |

Frames go straight from the producer to the registry. They never touch the reactive graph or the element tree, so a surface frame costs no effect, layout or paint: the compositor redraws from retained buffers and samples whatever the registry latched. Layers refer to surfaces by id, so a surface can be bound in more than one window.

### 12.2 Element API

```rust
use rouge::render::surface_channel;

let (producer, source) = surface_channel();
std::thread::spawn(move || run_producer(producer));
let status = signal(SurfaceStatus::Waiting);

surface(source)
    .w_full()
    .h(400.0)
    .rounded(12.0)
    .status(status)
    .fallback(|error| text(error.to_string()))
```

- `surface` is a taffy leaf with no intrinsic size that always owns a layer.
- Its **placement** is its physical pixel size, the scale and visibility. A surface is invisible when it or an ancestor is hidden or invisible, when ancestors clip it away entirely or it lies outside the window, when the window is occluded (`Ui::set_visible(false)`), or once it is removed.
- `.status(signal)` receives `SurfaceStatus::{Waiting, Live, Failed(SurfaceError)}`; `.fallback(view)` is mounted while the status is `Failed`.
- The element is an ordinary element for events, focus and hit testing.

### 12.3 Producer protocol

The protocol is data only, over channels in both directions:

```rust
pub fn surface_channel() -> (SurfaceProducer, SurfaceSource);

impl SurfaceProducer {
    pub fn cx(&self) -> Option<&SurfaceCx>;
    pub fn configure(&mut self, desc: BufferDesc) -> Result<(), SurfaceError>;
    pub fn acquire(&mut self) -> Option<(BufferRef, &SharedBuffer)>;
    pub fn submit(&mut self, frame: Frame);
    pub fn request_frame(&self);
    pub fn fail(&self, error: SurfaceError);
    pub async fn recv(&mut self) -> Option<SurfaceEvent>;  // also try_recv, blocking_recv
}

pub enum SurfaceEvent {
    Attached,
    Configure(SurfaceConfig),
    FrameRequest { target: Instant },
    Detached,
}

let (buffer, shared) = producer.acquire()?;   // render into shared.texture()
producer.submit(
    Frame::new(buffer)
        .region(valid_rect)              // the valid rectangle in buffer pixels; whole buffer by default
        .origin(Origin::BottomLeft)      // row order; TopLeft by default
        .alpha(AlphaMode::Opaque),       // Premultiplied by default
);
```

- `SurfaceProducer` is `Send`. The `SurfaceSource` goes to the element, which attaches it to the registry when the window syncs. Producers own their resources and talk to Rouge only through this protocol; nothing is shared behind a lock.
- `Attached` means `cx()` now returns a `SurfaceCx`: the app's wgpu `Device` and `Queue` and the adapter's `AdapterInfo` (so a producer can check it is on the compositor's GPU and report `SurfaceError::AdapterMismatch` otherwise).
- `Configure` carries a `SurfaceConfig` (physical width and height, scale, visibility), sent only when the placement changes. The registry stores the same type as the surface's placement.
- The producer owns the buffer pool. `configure` allocates it (a repeated call with the same description does nothing) and registers it with the registry; reconfiguring at a new size retires the old set. `acquire` hands out a free buffer, and a buffer returns to the free list on its own once the compositor no longer reads it, so producers never track releases. Frames name a buffer, not a texture; the registry imports a buffer set once.
- Fences, timed queues (`present_at`) and the `Presented`/`Discarded` events are deferred until a real producer needs them; see 12.5 and 12.6.

### 12.4 Buffers

- `producer.configure(BufferDesc::new(w, h).format(f).count(n).sharing(s))` allocates `n` `SharedBuffer`s (a wgpu texture plus, for native sharing, the OS object) through the attached `SurfaceCx`. Swap-chain allocation and recycling live in Rouge once, not in every producer.
- `Sharing::Device` allocates plain textures on Rouge's device, for producers that render with wgpu. It works on every platform, including the browser.
- `Sharing::Native` allocates memory a foreign API can render into: an IOSurface on macOS (`SharedBuffer::io_surface()`). `rouge_render::interop::IoSurface` also offers locked CPU writes for software producers and the global id for cross-process use. The import wraps `newTextureWithDescriptor:iosurface:plane:` with `wgpu::hal::metal::Device::texture_from_raw` and `Device::create_texture_from_hal`; failures are `SurfaceError`s, like every other surface failure.
- On Linux, native sharing is designed as DMA-BUF: `wgpu::hal::vulkan::Device::texture_from_dmabuf_fd` under `VULKAN_EXTERNAL_MEMORY_DMA_BUF`, with `SurfaceCx` advertising the importable DRM format and modifier pairs. DMA-BUF is chosen over opaque fds because it is a plain wgpu feature flag, it is what VA-API and Wayland already speak, and it does not require both sides to use the same driver build. Until it is built, `Sharing::Native` returns `SurfaceError::Unsupported` off macOS.
- Buffers are `Bgra8Unorm` or `Rgba8Unorm` holding sRGB-encoded bytes, matching the gamma-space scene, so no conversion pass is needed. sRGB-typed formats are rejected with `SurfaceError::Format` because sampling them would decode to linear light. Multi-planar YUV would be new sampling variants behind the same protocol.

### 12.5 Queues and pacing

- **Mailbox** is the only queue: at latch, the newest submitted frame wins. An older frame that was never sampled goes straight back to the producer's free list.
- **The compositor is the clock.** A producer calls `request_frame` instead of running a timer. After the next present, the registry answers with `FrameRequest { target }`, the predicted time of the following frame. Only visible surfaces get answers, and a pending request on a visible surface asks the host for a frame, so producers animate at display rate, stop when hidden or occluded, and a static producer costs nothing.
- **Deferred:** timed queues (`present_at` for video) and `Presented`/`Discarded` reports. Nothing in the tree produces timed frames yet; they return with the first video source, behind the same protocol.

### 12.6 Synchronization and release

- A submitted frame must be complete in GPU order: it was submitted on the `SurfaceCx` queue, or the producer already waited for its own GPU. The compositor never waits on a producer.
- **Deferred:** fences (`FenceSignal`/`Fence` and a frame's acquire state), until a producer that renders through another API needs them. The web view in section 20 is the expected first user.
- When a frame is replaced, its buffer is retired. It returns to the producer once all GPU work submitted so far has completed (`Queue::on_submitted_work_done`), because wgpu cannot see a foreign API writing into a texture it is still sampling. A buffer is read-only for the producer from `submit` until `acquire` hands it out again.
- `Surfaces::maintain` registers those callbacks and polls the device while any are outstanding, and `next_deadline` keeps the loop waking every 2ms until they fire, so an idle loop never strands a buffer. With three buffers a producer never waits on the compositor. In the browser, wgpu handles are not `Send`, so releases are sent at `maintain`; that is safe there because the only producers are device-shared and ordered by Rouge's queue.

### 12.7 Compositing

- The renderer samples the texture in place with a surface shader built on the effect prelude: the layer's transform and opacity, the rounded-rect SDF for coverage, and a UV mapping from the frame's region and origin. Bottom-left producers need no flip pass.
- Bind groups are cached per buffer and dropped when its set is gone, so a steady-state present allocates no GPU objects.
- The surface draws above the element's own background and border; `Opaque` frames ignore alpha.
- A surface layer is never itself rasterized. Every cache that contains a surface (a rasterized ancestor, a content effect, a backdrop behind glass) includes the current frame of each contained surface in its signature, so a new frame invalidates exactly those caches.
- During a resize the last frame is stretched to the new bounds until a frame at the new size arrives.
- When a surface element is removed, the producer gets `Detached` and its pending frames are discarded. A layer leaving with an exit animation keeps showing the last frame; the registry keeps the surface while any layer binds it, then releases its buffer.

### 12.8 One device, and the host contract

Imported textures belong to one wgpu device, and a producer may outlive a window. On native, the app owns one `Gpu` (which is `Clone`) and one `Surfaces` shared by all windows; all windows submit to one queue, so a release covers every window that sampled the buffer. In the browser, one device is shared by every canvas on the page and each mounted canvas owns a registry.

A host drives the registry around each composite and from its idle wakeup:

1. Before drawing a window: `ui.sync_surfaces(&mut surfaces)` sends attaches, placements, detaches and layer binds (a bind count kept on each registry entry); `latch()` picks up registrations and frames.
2. `Renderer::render(..., &surfaces)` draws.
3. After presenting: `presented(at, interval)` answers frame requests, then `maintain()` schedules releases.
4. When idle: sync and `latch()`. If anything changed or the registry's `needs_frame()` is true, redraw every window whose `Ui::has_surfaces()` is true; `next_deadline()` sets the wakeup.

`Surfaces::stats()` counts surfaces and frames submitted, latched, discarded and released.

### 12.9 Testing

`rouge_render::testing::TestSurface` is a real producer on Rouge's device: it configures its pool through `SurfaceProducer::configure`, renders known patterns with one pipeline and uniform buffer created on first use, and exposes `pump`, `present`, `present_flipped`, `request_frame`, `fail`, `config` and `free_buffers`; its builder sets the buffer count, sharing, format and size. `TestUi::surfaces()` and `Headless::surfaces()` provide a registry on a headless GPU. Pixel tests cover clipping, origin, alpha, regions, transforms, effects and glass over a surface, the mailbox, releases, frame requests, resize, exit and fallback, and IOSurface import on macOS. Tests assert that a new surface frame uploads no instance or image bytes and runs no layout or paint.

## 13. Events, input and focus

- `Ui` is the input boundary; there is no separate event enum. Hosts call `pointer_moved`, `pointer_down`, `pointer_up`, `pointer_cancel`, `scroll`, `scroll_drag`, `scroll_release`, `touch`, `key_down`, `key_up`, `ime`, `file_drop`, `set_modifiers`, `set_focused`, `set_visible`, `set_appearance`, `set_size` and `set_device_scale`, with positions in logical (zoomed) coordinates.
- Hit testing walks the element tree using the compositor's current (animated) layer transforms, so it always matches what is on screen. Clipping ancestors, unseen elements and fully transparent layers are excluded; `.pass_through()` lets events fall through an element.
- Dispatch bubbles from the deepest element to the root; `event.stop_propagation()` ends it. Handlers (`on_click`, `on_pointer_down/move/up`, `on_scroll`, `on_key_down/up`, `on_focus`, `on_blur`, `on_hover`, `on_press`, `on_ime`, `on_drag`, `on_file_drop`) run untracked in a batch, with the scope that registered them as owner.
- `event.capture()` in a pointer-down handler routes moves and the release to that element. `.on_drag` captures once the pointer moves 3px, reports start, move and end with translation and velocity, and replaces the click. `.drag_axis(DragAxis::Horizontal)` (or `Vertical`) locks a drag to one axis: it does not begin when the first 3px run mostly along the other axis. Pointer events carry element-local coordinates, modifiers and click count.
- Hover, active, focus and disabled states are tracked by the framework and drive style variants. `on_press(f)` hears the active state change: `true` on a primary press, `false` on the release wherever the pointer ends or on a cancel, so a press can drive compositor properties such as scale. A disabled element and its descendants get no pointer handlers, focus or state styles.
- **Focus**: tab order with `tab_index`, traversal confined to the innermost `.focus_scope()` (modals, popovers), `.autofocus()` with focus restored when the scope goes away, `FocusHandle` for programmatic focus, and `focus_visible` for keyboard focus. A click on something unfocusable clears focus unless an ancestor is `.preserve_focus()`. With nothing focused, keys go to the window's root view or an element marked `.root_host()`.
- **Keymap**: `.bind_key(chord, action)` and `.on_action(|a: &Save| ...)` live on elements; the deepest binding on the focus path wins and the deepest handler for that action type runs. Menu shortcuts are bound window-wide by `Ui::bind_menus`, consulted after element bindings; Tab moves focus last. Chords parse from strings such as `"secondary-k"` (`cmd`, `command` and `secondary` mean the platform command key). `use_binding::<T>()` and `Ui::binding_for::<T>()` report the effective chord, and `Ui::dispatch_action` invokes the deepest handler directly.
- **Clipboard** is the `Clipboard` trait. Reads are `PasteRequest`s answered now or later with text or media (files, PNG images); an answer that arrives after an edit, focus change or remount is dropped. `MemoryClipboard` (with `deferred()`) fakes it in tests. Native uses arboard; the browser uses DOM events (section 14.3).
- **Touch** (`Ui::touch`): a press goes through pointer dispatch, so taps click and show active styles; after 8px a touch whose press has no drag handler becomes a scroll pan that flings on release; touches leave no hover. An axis-locked drag shares the surface with scrolling: the touch pans instead when it runs along the other axis, or when a scroller under the finger can still move in the direction of travel, so a row swipes sideways inside a list that scrolls and a sheet is dragged only once its content has reached its edge. A drag with no axis always wins, as before.
- **Query and test helpers** on `Ui`: `bounds_of(test_id)`, `bounds_of_all`, `named_bounds` (every element with a test id, in tree order), `text_of`, `text_bounds`, `text_index_at`, `bounds_at`, `cursor`, `accepts_text`, `text_cursor_area`.

## 14. Platform hosts (`rouge`)

### 14.1 App and windows

`App::new().window(options, root).run()` is the entry point on both hosts. `App` also takes `menus`, `fonts` and (with `tokio-rt`) a Tokio handle; each `window` call opens a window with its own `Ui`. `WindowOptions` covers title, size, minimum size, resizable, maximized, decorations and, in the browser, the canvas id. The window clears to `Ui::clear_color()`: the background of the root view's outermost painted element, or the theme's `canvas` when nothing paints one.

Each `Ui` provides a `Copy` `WindowHandle` in context: reactive `title()`, `set_title`, `navigate`, `minimize`, `zoom`, `close`, `start_drag`, `start_resize`, `quit`, `focus`, `dispatch_action` and `copy_text`. Commands queue on the window and the host applies them. `appearance()` and `window_focused()` are reactive signals fed by the host. `route()` is a `ReadSignal<String>` of the window's route: the host sets it with `Ui::set_route`, and `WindowHandle::navigate` changes it and sends `WindowCommand::Navigate`. In the browser the route is the URL fragment without `#`; on native it lives only in the window.

**Persistence** (`persist` feature). `Storage` is an async string key-value trait in `rouge::persist`: `read` and `write` return boxed futures, so a networked store fits it. A root view provides one through context with `provide_storage(app_storage(name))`; without one, `persisted` does nothing. `persisted(key, source)` takes a `Signal<T>` or a `KeyedList<K, V>` of serde types and spawns one task in the caller's scope: it awaits the saved JSON and loads it, then serializes on every change and writes the latest JSON at most every 400ms, one write at a time. When the scope is disposed it hands any unsaved JSON to `Storage::write` on a detached task, so a store should accept the value when `write` is called and use the future only to report completion. `FileStorage` reads with `spawn_blocking` and writes one JSON file per key on a writer thread that is joined on drop; `LocalStorage` prefixes keys in `window.localStorage`; `MemoryStorage` is the fake for tests, with `get` and `insert`.

Menus are the shared `Menu` / `MenuItem` model: typed actions or callbacks, shortcuts, separators, group labels (`MenuItem::label`, drawn natively as a disabled item) and submenus, with optional `id`, `icon` and `destructive` styling that native menus ignore. Both hosts apply one policy: an Edit menu is added when absent (and, on native, a Window menu), so cut, copy, paste, select-all, undo and redo shortcuts always reach the focused editor; an in-window menu bar renders only when the app supplied menus. On macOS the menus go to the native menu bar.

### 14.2 Driver and native host

`Driver` is host glue: it opens the window's `Ui` and its swapchain `Target` (the wgpu surface configuration, a non-sRGB format, vsync), holds the frame token, resizes, steps and commits (section 9.5), dispatches window commands and menus, tracks cursor and text-input changes, and runs the profiler. It never touches a `Renderer` or the surface registry. Both hosts translate platform events into `Ui` calls and apply the driver's outputs.

**The compositor actor.** `CompositorActor` (`rouge_core`) owns the `Surfaces` registry and, per window, a `Compositor`, a `Renderer`, a `Target` and a frame clock (the last present plus the refresh interval). It is a state machine with no thread or clock of its own: `handle(message)` applies a `Message` (open, a `WindowConfig` of size, refresh interval and visibility, a `Commit`, detach and attach of the window's `Target`, or close); `next_wake(now)` reports the earliest due window that has an owed acknowledgement, a new surface frame or a pose that changes after its last present (`LayerModel::next_change`), so a held keyframe wakes the compositor only at its next step; `frame(now)` draws the due windows, sampling each model at `now`, and then emits `Presented` only for windows that owed one. An animation the UI is not watching therefore costs the UI no message and no wake. A hidden window acknowledges a commit with content at once and holds an empty one until it is visible. A resize makes its window due at once. A failed acquire backs off for one interval, and a skipped one still acknowledges, so the token is never lost.

Hosts reach the actor through a mailbox (`rouge_core::mailbox`): a `Sender<Message>` that every `Driver` holds, and a receiver of `(WindowId, CompositorEvent)`. `Mailbox::turn(actor, now)` drains the messages, draws the due windows and forwards the events.

Two things keep the replica faithful to the UI that wrote it. A `Commit` carries the scale its draw lists were painted at (`Ui::scale()`, the device scale times the zoom) whenever that changes, so the window is composed at the scale of its content and a zoom lands in the same frame as the repaint it caused. And the UI and the compositor measure time from one epoch: the host takes a single `Instant`, gives it to every window's `Ui` (`Ui::set_epoch`) and to whichever loop calls `turn`, so an animation started at a UI time is sampled at that same time.

The hosts differ only in who calls `turn`:

- Inline: the host owns the actor and calls `turn` itself. The browser host does this, and the native host does with `ROUGE_COMPOSITOR=inline`.
- Thread: `rouge_core::spawn` runs the same `turn` in a loop on a `rouge-compositor` thread, which waits on the one inbox (commits, window messages and surface wakes) until the next wake and wakes the event loop only when it sent events. Values are moved, not encoded. The native host uses it by default, so waiting for a drawable never blocks the UI thread. On resize the host steps, commits and waits up to two frames for the `Presented`. Closing a window keeps the winit window alive until `Closed` comes back, and at exit the host sends `Shutdown` and joins the thread before dropping its windows; the thread hands its render targets back so the swapchains are dropped on the main thread. A panic on the compositor thread becomes `AppError::Compositor`.
- `TestUi` holds the actor directly, with the test clock and an offscreen `Target`. `TestUi::composite(seconds)` draws a compositor frame ahead of the UI without stepping it, which is how a stalled UI is tested without threads or sleeps.

The native host (`platform::native`) runs winit on macOS, Wayland and X11. What differs per target sits behind the `WinitPlatform` trait, whose defaults are the desktop behaviour, so the shared runner carries no `target_os` checks. One `Runtime`, one `Gpu` and one compositor actor serve all windows. The loop waits (`ControlFlow::Wait`, or `WaitUntil` the earliest compositor wake or registry deadline), steps a window's UI on `RedrawRequested` only when it holds the token and wants a frame, and gives the inline compositor its turn in `about_to_wait`; occlusion is reported with `set_visible`. macOS gets a native menu bar through muda (with About, Hide, Hide Others, Show All and Quit, and a Window menu added when absent); elsewhere the menus render as Rouge's `menu_bar`, because muda's Linux integration needs GTK-owned windows. Client decorations, or a compositor that declines server decorations, wrap the root in `window_chrome` (drag, double-click zoom, controls, resize edges). `platform::open_url` is behind the `open-url` feature.

**Text input contract.** Every host edits the focused field through one surface on `Ui`. `Ui::editor()` returns an `Editor` for the focused editable: its identity (`id`, so a host can tell a late update for one field from an edit to another), text (`slice` clamps a range to character boundaries), directed selection, marked (composing) range, `InputHints`, and geometry and movement from the Parley layout that draws it (`caret`, `rects` for a range, `index_at` a window point, `moved` by one `Movement`). `Ui::edit_text(EditTransaction)` applies an ordered list of range replacements, a selection and the marked range as one atomic edit; the marked range it carries is authoritative, a composition is one undo step when it commits, and out-of-range or misaligned offsets are clamped to character boundaries. `EditTransaction::between(before, after)` turns a whole-state update into the smallest replacement. Offsets are UTF-8 bytes in core; `text::utf16_offset` and `text::byte_offset` convert at a host boundary. `Editable::input_hints` (and `text_input().input_hints`) carries the content type, the enter key's action and autocorrection to the platform keyboard. `ImeEvent` stays the desktop and browser input; a preedit or commit is lowered to the same transaction, as are typing, erasing, cut and paste, so replacement, sanitizing, the marked range and undo have one path.

**iOS** runs on the winit host with one addition (`platform/ios/text_input.rs`): a `UITextInput` view attached to the window's view, first responder only while an `editable` has focus. UIKit calls it on the main thread, where the `Ui` also lives, so it answers queries from `Ui::editor()` and applies edits with `Ui::edit_text` directly. Its `UITextPosition` and `UITextRange` hold UTF-8 byte offsets, converted to UTF-16 only where UIKit passes a count, and directional moves use `Editor::moved`; the runner holds the `Driver` in an `Rc<RefCell<..>>` for that, and the host notifies the input delegate of changes UIKit did not make only after its own borrow has ended. Return and delete go through the key path. The keyboard's height comes from the view's `keyboardLayoutGuide`, reported as `keyboard_inset()` when it rises above the bottom safe area. An element that listens for `ImeEvent` without being an `editable` still gets winit's key input.

**Android** has its own host (`platform/android/host.rs`) on `android-activity` with `GameActivity`; winit is not in the Android build. The app is a `cdylib` whose `android_main` passes its `AndroidApp` to `App::android`; `run` returns `AppError::Platform` (`platform::Error::NoActivity`) without one. An activity has one native window, so an app that opens more than one gets `platform::Error::MultipleWindows`. The host polls the activity for lifecycle and input events, steps the `Driver` when a frame is due and sleeps until the next deadline. What differs from desktop:

- **Surface lifecycle.** Android destroys the native window whenever the activity leaves the screen. On `TerminateWindow` the host cancels the active touch, sends `Message::Detach(window, reply)` and blocks until the actor has dropped the window's `Target` and replied, so the swapchain is gone before the callback returns. The actor keeps the window's `Compositor`, `Renderer` and layer replica; while detached it acknowledges commits at once, as a hidden window does, and asks for no wake. On the next `InitWindow` the host creates a surface for the new native window, builds a swapchain with the driver's current size and the format the window's `Renderer` was built for (failing with `AppError::UnsupportedSurface` if the new surface cannot use it), sends `Message::Attach(window, target)` and steps once so the retained scene is presented. The `Ui` is never rebuilt.
- **Text input.** The IME calls on the Java UI thread and the `Ui` lives on the `android_main` thread, so GameTextInput's Java-side state is the one projection of the focused field. When the field or its content changes the host publishes text, selection and compose region (and the editor info from `InputHints`); each `TextEvent` is diffed against the last published state into one `EditTransaction`, and an event equal to that state is dropped as an echo. GameActivity delivers touches before text, so an event is applied only while the focused editor is still the one that state was published for, with the same text; otherwise it is dropped and the field is published again. GameTextInput keeps the text in a 64 KiB buffer of modified UTF-8 and does not check its length, so the bound is held on both sides: a field that does not fit is published as an empty `TYPE_NULL` editor whose committed text is inserted at the selection, and the generated activity replaces an oversized IME state with a marker that makes the host drop it and publish the field again. The Next and Previous editor actions move focus; the others are sent as Enter. A tap on a focused field shows the keyboard again after the system has hidden it.
- **Other input.** Touches go to `Ui::touch`, as on iOS, and both report `InputKind::Touch` to `presentation()`; the desktop hosts report `Pointer`. Hardware keys go through the device's key character map. Back arrives as Escape, so it dismisses overlays that handle it; when nothing does the host moves the task to the background (`moveTaskToBack`). Android keeps the process after an activity is destroyed, and a second `android_main` in it would find the first one's state, so when the activity is destroyed or the app quits the host kills the process (after `finishAndRemoveTask` when the app asked to quit) instead of returning.
- **Insets.** The generated activity draws edge to edge. On `InsetsChanged` the host reads typed insets through JNI: system bars and the display cutout feed `Ui::set_safe_area`, and the IME's bottom inset feeds `Ui::set_keyboard_inset`. The manifest uses `adjustResize`, which is what makes the system report IME insets.
- **Accessibility.** `accesskit_android` is attached to GameActivity's surface view with its embedded dex. Its handlers run on the UI thread and forward requests over a channel to the host, which answers with the same `Ui::accessibility_tree`, `accessibility_update` and `accessibility_action` calls as the winit host.
- **System services** (`platform/android.rs`, with the host the crate's only `unsafe` besides iOS) go through JNI on the activity: the clipboard (`ClipboardManager`, text only), the display's refresh rate, window insets and `open_url` (an `ACTION_VIEW` intent). Dark mode is read from the activity's configuration, and `app_storage` writes under the app's internal data directory. `stderr` reaches logcat under the `RustStdoutStderr` tag.
- **Build.** An Android app needs the generated `RougeActivity` (a `GameActivity` subclass) and the `games-activity` library, so it is packaged with Gradle through `rouge-framer`, for API 30 and later on arm64. `mise run platform:android:check` type-checks the target through `cargo-ndk`; `mise run run smoke android` generates the Gradle project for the mobile smoke app, builds, installs and starts it and streams its log.

### 14.3 Browser host

The browser host (`platform::web`) is part of the `rouge` crate and deliberately custom (section 3.2). The same `App` code runs on `wasm32`: `run` spawns the mount and returns, and the app lives for the page. `App::mount().await` returns a `Mounted` handle instead, for pages that start and stop the app; dropping it unmounts every window and cancels its pending wakes. Each mounted canvas owns its reactive runtime; the runtime's waker finds its host by id and coalesces wakes into one microtask. `WindowOptions::canvas(id)` uses that `<canvas>`, or fills the element with that id with a new one; without it the canvas covers the page. A mount error goes to the console and into the page.

- **GPU.** The host asks the browser for a WebGPU adapter before it creates the wgpu instance and falls back to WebGL2 when there is none; the renderer's limits fit both (section 9.2). `?rouge_backend=gl` picks WebGL2 where WebGPU is available. On WebGPU one device per page is shared by every canvas; a WebGL2 context belongs to its canvas, so there each canvas has its own device.
- **Frames.** `requestAnimationFrame` is requested only when the driver wants a frame, a visible surface has or wants one, or the canvas resized; an idle page requests nothing. The backing size comes from a `ResizeObserver` on `device-pixel-content-box` (content box times `devicePixelRatio` where unsupported), so the canvas is never resampled. The canvas rectangle is cached so pointer events do not force a browser layout.
- **Input.** Pointer events with fractional coordinates and pointer capture; touch goes to `Ui::touch`. The input kind for `presentation()` is read once at mount from the `(pointer: coarse)` media query. Wheel deltas convert from line and page units and get no Rouge fling, since browsers deliver momentum; ctrl-wheel is left to browser zoom. Keys map from `key`, using `code` to recover the unshifted key so chords match native. Text arrives through a **hidden textarea placed at the caret**: `beforeinput` and `input` commit, and composition events become preedit and commit. `prefers-color-scheme` drives `Appearance`; focus, blur and `visibilitychange` drive `set_focused` and `set_visible`; `hashchange` drives `set_route`, and `Navigate` sets `location.hash`.
- **Clipboard.** Keyboard copy, cut and paste use the DOM's **native clipboard events**: the host dispatches `EditAction::{Copy, Cut, Paste}` while `WebClipboard` captures the text written or supplies the text pasted, so editing goes through the same `PasteRequest` path as native. Other writes use `navigator.clipboard.writeText`; a menu-initiated paste uses `readText`, which may prompt.
- **Window commands.** `SetTitle` sets `document.title` and `Focus` focuses the canvas; the rest do nothing.
- **Listeners.** One listener per event target (page, document, canvas, hidden textarea, media queries) dispatches through a single `match` on the event type, mirroring the native host's one `window_event`.
- **Build.** `mise run platform:web:check` type-checks the browser target, `mise run platform:web:test` runs browser tests in headless Chrome, `mise run platform:web:build` builds an example with the `wasm-release` profile (fat LTO, `wasm-opt` when installed), and `mise run run gallery web` serves the component gallery.

## 15. Widgets (`rouge-widgets`)

Built strictly on the public API, which keeps that API honest. Every widget follows the constructor-plus-methods rule (section 6.2): every property that can change after mount accepts a value, signal or closure, an editable value is a `Signal` (`checkbox` takes a `Signal<bool>`, or a `Signal<CheckState>` for tri-state), and `dynamic` is only for structural replacement. Controls are keyboard-operable by default through `on_activate`. There is one component set:

- Controls: `button` (`.loading(signal)` keeps the width, shows a spinner and ignores input; `ControlSize::Large` is the 44 px touch size, and a button with no explicit `control_size` is `Medium` with a pointer and `Large` on touch, as is an `icon_button`, whose glyph grows from 16 to 20 px; `.confirm(Hold::seconds(1.0))` fires only after being held, by pointer or by Space or Enter, while a fill bound to a `Motion` grows across it), `icon_button` (which takes the same `.confirm(Hold)`), `toggle`, `checkbox`, `radio`, `slider` (`.format(f)` adds a glass value tip on hover and drag), `segmented_control` (a `segment` with an icon shows its label as a hint unless it is `.labelled()`, and `.badge(view)` pins a view to its top right corner), `text_input` (single-line or `multiline`), `inline_edit` (double-click starts an edit; `.begin_editing()` mounts it already editing, for a rename started from a menu), `kbd`, `key_hints([(keys, action)])` (a wrapping row of keys in a `kbd` beside what they do, for a picker's footer), `field` (a label, a hint and an error around one control; a set error replaces the hint and gives the control inside a `Role::Destructive` border through context), `number_field` (a `Signal<f64>` with `range`, `step` and `suffix`; drag the label to scrub, type to edit, arrows to step), `tag_input` (tags in a `KeyedList<String, String>`), `code_input` (a fixed-length code drawn as cells over one `editable`), `search_field` (a search glyph, the entry and a clear button in one frame; on touch a Cancel button joins it while it is focused or filled, and `.scopes(selected, labels)` shows a segmented control beneath it for as long), `chip` (a filter that is tinted while `.selected`, with an optional `.count`) and `chip_row` (a row of them that scrolls sideways), `wheel(selected, labels)` (a column that turns under a drag, settles with one choice in its middle row from the position plus a fifth of a second of velocity, and takes taps and arrows) and `date_picker(value)` (three wheels over a `Signal<Date>`; `Date::new` clamps to a real day, so February never keeps a 31st). On touch the controls resolve a larger size the way `button` does: `toggle` is 52 by 32, `checkbox` and `radio` are 24, each inside a 44 px target, `slider` has a 28 px thumb in a 44 px track, `segment` is 36 px in a 44 px control, `text_input`, `select` and `number_field` are 44 px tall, and `number_field` trades its scrubbing label for minus and plus buttons that repeat while held.
- Structure: `scroll_view` and `vertical_scrollbar` (thumbs a whole number of units long that track the compositor offset, drag and track jumps; `.refresh(signal)` sets the signal when the content is pulled 48 px past its top and shows a spinner until the caller clears it), `tree_row`, `list_row` (`.action(row_action(label, f).icon(..).destructive())` adds actions (`.icon_only()` draws a square icon chip named by its tooltip; a swipe still shows the label) that fade in at the trailing edge on hover or keyboard focus with a pointer and are revealed by a sideways swipe on touch; `.trailing(view)` puts a label such as an age at the trailing edge, where the actions lie over it while they show instead of reserving room beside it; rows are 44 px tall on touch; `.leading(view)` puts a view before the content, `.detail(text)` a quieter second line under it, and `.chevron()` a chevron at the trailing edge), `list_group` (an elevated card of rows under an optional `.title`, divided by rules that take no room, in which rows inset themselves; `icon_tile(icon)` is the tinted square for a row's leading slot), `section_header`, `disclosure` (`.body(view)` for a collapsible panel, `.accordion(open, key)` to keep one of a group open), `tabs` (one sliding underline, panels cross-fade; `.icon_tab(icon, label, view)` adds an icon; `.bar()` marks tabs that are a window's main navigation; `.badge(count)` and `.dot(on)` mark the tab added last, and `.accessory(view)` adds a slot for something that outlives the screen), `separator`, `resizable_panel` (`.open(signal)` collapses it) and `panel_toggle`, `action_bar` (`.action(row_action(label, f).icon(..))` for icon actions beside its children, the same action type `list_row` takes), `nav_bar(title)` (a 44 px bar with a centred title, a `.leading` slot and `.trailing` actions; `.large()` shows the title in a row of its own, which folds into the bar once the scroller given to `.scroll(state)` has moved 24 px and returns at the top; inside a `nav_stack` page it shows a back button), `nav_stack()` (`.page(pattern, |params| view)` adds a screen; the window's route shows the screen of each of its prefixes that has one, the longest on top, so `/thread/42` stacks the thread on the inbox; a pushed screen slides in on a `Motion`, the one beneath shifts and dims from the same motion and stays mounted, and Escape, the back button or a drag from the leading 32 px pops by navigating to the path beneath), `pager(page)` (pages that snap under a horizontal drag one at a time, with `.peek` showing the neighbours) and `page_dots(page, count)`.
- Data: `data_table(rows)` takes columns as closures over the row type (`column("Title", |track: &Track| ..).flex(1.4).sortable()`), with rows from a `KeyedList` or from `table_rows(count, row)`. Its body is a `virtual_list`, so it mounts only the visible rows; `.selection(signal)` holds the selected row's key, `.sort(signal)` is written by sortable headers (the caller reorders its own data), and `Column::width(signal)` adds a drag grip. The header sits above the scroller instead of being scroll-linked inside it. `sortable(list, row)` reorders a `KeyedList` by drag or by Alt with an arrow: the dragged row follows the pointer on a `Motion`, the rows it passes shift with `Pace::Layout`, and the list is written once on release (`KeyedList::move_to`).
- Overlays: `sheet(open, content)` (a glass panel that rises from the bottom edge over a scrim; `.detents([Detent::Medium, Detent::Large])` names its resting heights and without them it hugs its content; a drag follows the finger on a `Motion`, a release picks the nearest detent, or closed, from the position plus a fifth of a second of its velocity and settles with `Pace::Gesture`; the scrim's opacity is mapped from the same motion; arrows on the focused grabber step between detents and Escape closes), `popover`, `tooltip` and `Hint` (`.tooltip(label)` on any view, with `.shortcut(chord)` shown in a `kbd` and `.content(view)` for a rich body), `context_menu` (`.dropdown()` for a click-opened menu), `select`, `search_picker` and `picker_list`, `backdrop`, `modal`, `alert(open, title)` (a small centred glass panel at every width, with a `.message`, `.action(row_action(label, f))` buttons, each of which can be `.destructive()`, and a `.cancel(label)` that Escape chooses; presses outside it do nothing, and three or more answers stack), `fab(icon, label)` (a 56 px round button in `Material::Pigment` that places itself in the bottom trailing corner of its container, above the safe area, the keyboard and any bottom chrome; given `.action(row_action(label, f).icon(..))` it opens a speed dial of labelled actions over a scrim instead), `keyboard_bar()` (a floating strip above the soft keyboard, shown only while `keyboard_inset()` is not zero, with optional previous and next buttons, a `.trailing` slot and a Done button that takes focus from the field), `menu_bar`, `window_chrome`.
- Feedback: `progress`, `progress_ring` (a ring that fills clockwise in the colour it is given, as large as the box it is given), `spinner` (a quarter arc circling a faint track, 20 px unless it is given a size; `.track(fill)` and `.arc(fill)` recolour it, and at the size of a round button it frames a glyph in the middle), `notification` (optional `.title` and `.action`), `empty_state` (optional `.action`), `muted_text`, `badge` (a label tinted by a `NoticeTone`), `avatar` and `avatar_group` (initials or an image; a group overlaps and counts its overflow), `skeleton` (a placeholder block that shimmers), `breadcrumb` (links and a current item that clips when there is no room for it), and `toast`. `Toasts::install()` at a window's root gives the window its own queue through context, and the returned handle is the view that draws the stack in the overlay root; `toast("Saved").tone(..).action("Undo", f).show()` adds to the queue of the window it is called in. The stack fans out and pauses its timers while hovered. `.banner()` shows a toast in a second stack at the top of the window.
- With features: `markdown` (on `rouge-markdown`) and `syntax_spans` (on `rouge-syntax`). The `markdown` feature also brings `transcript(list, |row| Message)` (user bubbles in `Material::Pigment`, assistant replies through `markdown(stream)`, on a measured `virtual_list` that follows its end) and `composer(draft)` (a multiline input that sends on Enter and shows a spinning arc while `.busy(signal)`; supplying `.on_stop(f)` turns that indicator into a Stop button). `markdown(document)` or `markdown(stream)` (a `Signal<MarkdownStream>`) renders a document and keeps its rows in sync as the stream grows; `.on_link(f)` hears link clicks, colours follow the theme unless `.palette(...)` or `.syntax(...)` overrides them, and the code-block copy button writes to the window's clipboard. `MarkdownRows` and `MarkdownPaint` expose the same row projection for virtualized and annotated views; the widget hands its rows to `each` by signal, so a sync never copies the row list.

Spacing is a 4 px grid that is part of the set, not a convention. `tokens::UNIT` is 4, and `tokens::Space` is the scale of nine steps named by unit multiple (`X1` is 4, `X2` 8, then 12, 16, 24, 32, 48, 64 and `X24` is 96); it converts into a `Length`, so `.gap(Space::X2)` works wherever a length does. Inside `rouge-widgets`, padding, margins and gaps take a `Space`, and `scripts/style-check` rejects a number there as it rejects `Corners::all`; a 1 px edge is `tokens::HAIRLINE`, and a bordered box whose height follows its content subtracts it from its padding so the box stays on the unit. `column_grid()` is twelve equal columns and `grid_cell(span)` a cell that pads itself by `tokens::GRID_MARGIN` (24 px, half of the 48 px `GRID_GUTTER`), so content in every cell starts on a column edge and a divider between cells sits in the middle of a gutter; its span and its optional `start` column are reactive, so a centred column can widen and narrow with its window; `reading_span(width, max_width)` is the widest even span whose lines stay within `max_width` in a grid that wide, and `reading_cell(span)` centres that span, so a document or a transcript starts its text on a column edge at any width; cells take the full width in a compact window. `grid_overlay(visible)` draws the columns over any `relative` container for checking a layout and passes input through. `tests/tokens_test.rs` mounts every control and fails if one is not a whole number of units tall, `tests/markdown_test.rs` does the same for every kind of Markdown block after every other (block gaps are `Space` steps and bordered code, table and rule rows give their hairlines back), and `crates/rouge/tests/gallery_test.rs` does the same for every cell of the gallery and checks that each starts on a column.

Components read colours from the theme and metrics from `tokens`, size themselves in pixels, scale with `InterfaceSize` (which sets the window zoom to `size / tokens::BASE_SIZE`), ship Inter and Lilex through `bundled_fonts()`, and draw Lucide icons through `Icon`. Elevated surfaces take one `Material`: `Glass` for panels that float above the page (the only material with a drop shadow), `GlassControl` for glass at control size, `Surface` for an opaque card in the page's flow, `Pigment` for a flat fill of `Role::Accent` with `Role::AccentForeground` text (the picker's highlight), `Lens` for floating glass that bends and splits what is behind its edge, `Clear` for floating glass with no tint or blur, and `Bare`. All four glasses share the fixed overhead rim; none follows the pointer. Controls are flat: a primary button is a fill of `Role::Accent`, secondary and destructive buttons draw a hairline edge, and none casts a shadow. Motion uses `tokens::Pace` (`Press`, `Move`, `Enter`, `Exit`, `Layout`, `Gesture`), which converts into a `Timing` wherever one is accepted, so no widget names a spring or duration of its own; buttons and icon buttons scale with `Pace::Press` while pressed. Overlays (menus, popovers, tooltips, hover cards, the slider's tip) scale and fade in with `Pace::Enter` and out with `Pace::Exit`; a modal's panel scales in while its scrim fades in with a `BackdropBlur`. An indicator that follows a selection (the segmented control's pill, the tab underline, the picker's highlight) is one layer moved with `LayoutTransition::new(Pace::Move).size()`. `tokens::MotionScale` is a context value: `MotionScale::None.provide()` at a window's root makes every `Pace` built under it an instant change, and the returned signal is its setter. A `Pace` reads the scale when a component is built, so a change applies to what is built afterwards. Two shader states are `ShaderEffect`s with a clock uniform: a press ripple from the pointer position (`button(..).ripple()`) and the skeleton's shimmer. The ripple lives on an overlay layer that is mounted only while it runs, so an idle window draws no frames; a skeleton shimmers for as long as it is mounted. Both multiply by the element's coverage, so nothing is drawn outside its bounds, and neither runs under `MotionScale::None`. Popovers and menus render in the window's overlay root behind one dismiss layer that closes on an outside press or Escape; menus share one popup renderer with keyboard navigation and a focus policy (`MenuFocus::Take` or `Preserve`). `RovingFocus` moves focus through a list with arrows, Home and End. On a handheld (`Presentation::is_handheld()`) the same constructors present differently: `popover`, `select`, `context_menu` and menu-bar menus open as a content-height sheet instead of an anchored panel, `modal` is a `Detent::Large` sheet that reports `on_dismiss` once it has slid away, and `tabs` built with `.bar()` draws a floating glass bar above the bottom safe area instead of the underlined strip (tabs inside a page keep their strip), which hides while the keyboard is up; on touch at a regular width the same tabs stand in an 80 px rail at the leading edge. An `action_bar` floats in the tab bar's slot: with icon actions it takes the tab bar's place, and with buttons it sits above it and they share its width. A side `resizable_panel` given `.open(signal)` is a glass drawer over a scrim, closed by a press outside it or a drag towards its edge. Toasts span the window one at a time and are swiped away, sideways or back the way they came. A long press of half a second opens a `context_menu` on touch. The floating pieces register how far they reach from the bottom edge in one registry per window (`window_scope()`), in tiers (tab bar, keyboard bar, action bar), so each sits above the tiers below it and `fab` and the toasts clear them all. A change of width class rebuilds only that chrome. `rouge_widgets::testing::component_test()` builds a `TestUi` with the bundled fonts.

## 16. Standalone crates

These four crates have no UI or GPU dependencies (section 3.2).

- **`rouge-reactive`**: section 5.
- **`rouge-theme`**: `Rgba`, `UiPalette<T>` (the colour roles, listed once; `light()`, which is also `Default`, and `dark()`), semantic styles for control, selection and notice states, `ModalSize`, `MarkdownPalette`, `Appearance`. Widget-only vocabulary (`ControlSize`, `CheckState`) lives in `rouge-widgets`.
- **`rouge-syntax`**: Tree-sitter highlighting and symbol tags over the arborium grammars. One table lists every language once (name, aliases, extensions, special file names, grammar) and resolves a hint (an id, alias, fence language or path) to a `u8` language id; injections resolve to the same ids. `SyntaxStream::append` is the one parsing path; `analyze(language, source)` is a stream fed once. A line's highlights are one slice of `(start, end, HighlightId)`; the stream reuses its flattening buffers across appends. Work is counted per stream (`SyntaxStream::work_stats`) and carried by each `SourceSyntax` (`work_stats`), so callers sum the analyses they own rather than read a global counter. `SyntaxTheme` keeps only the per-`HighlightId` styles; `capture_style` resolves a capture name against a capture map for callers that keep one.
- **`rouge-markdown`**: GFM documents (pulldown-cmark, with bare URL and email linking) with blocks, selectable targets (id, kind, source range) and source styles; code blocks carry `rouge-syntax` highlights and derive their language from the fence info. Inline content has one representation, `MarkdownText` (text plus mark and link ranges), built in a single pass over the parser's events with a mark stack and shared by blocks as `Arc<MarkdownText>`; bare links are found once per plain-text run. `MarkdownStream` is the incremental parser: `push` appends and reports what changed, blocks settle once nothing appended can change them, and in a trailing list every item before the open one settles, so only the open item is re-parsed. Reference definitions in the settled prefix are kept as parsed values and resolved through the broken-link callback, so re-parsing the tail never copies the prefix. `MarkdownDocument::parse` runs the same tail parser over the whole source. Labels, outlines and other review projections are computed by the application from blocks and targets. A code block's symbol tags are extracted from its syntax tree only when first read.
- **Parsing off the UI thread.** `MarkdownStream`, `MarkdownDocument::parse` and `SyntaxStream` are plain data and `Send`, and they parse wherever the caller runs them. For large inputs, parse in `spawn_blocking` and publish an `Arc` snapshot into a signal: `let parsed = spawn_blocking(move || Arc::new(MarkdownDocument::parse(source)));` then `spawn(async move { if let Ok(document) = parsed.await { doc.set(document) } })`. For a stream that grows, keep the `MarkdownStream` on a worker that receives appends over a channel and sends back an `Arc` of each new revision through `signal.sender()`. Highlights are already shared as `Arc`s, so publishing a snapshot copies no text. `rouge-markdown` and `rouge-syntax` stay renderer-neutral; this needs no API of theirs.

## 17. Diagnostics and testing

- **Counters.** `Ui::stats()` returns cumulative `FrameStats`: layouts, relayouts, paints, layers painted, patches, style resolutions, elements created and removed, element and layer counts, `TextStats` and `ReactiveStats` (nodes created and disposed, effect and memo runs). Subtracting snapshots gives the work in between, which is how tests assert that scrolling and animation do no layout or paint. `Renderer::stats()` reports instance and image bytes uploaded, draw calls, groups and group passes, backdrop passes, blurs and reuses, and surface draws. `Renderer::memory()` reports the bytes of every texture and buffer the renderer holds, and `TestUi::memory()` reads it for the test window.
- **Tracing.** Rouge emits `tracing` spans: `update`, `publish`, `layout`, `hover`, `paint`, `scene`, `commit`, `composite`, `acquire`, `encode`, `present`. On native, `ROUGE_PROFILE=<file>` writes a Chrome trace with one event per frame carrying that frame's `FrameStats`, `ROUGE_BENCH=scroll:<px per frame>:<frames>` scrolls the window and prints frame-interval percentiles, and `ROUGE_EXIT_AFTER_FRAMES=<n>` closes the window after n frames. All of these are read by `ProfileSettings`; a profile logs its own summary when it ends, and the environment description is only formatted while profiling. In the browser, `?rouge_profile` records spans as `performance.measure` entries and `?rouge_bench=...` mirrors the bench. `WGPU_BACKEND` and `WGPU_POWER_PREF` choose the adapter.
- **Benchmarks.** `crates/rouge-bench` is an unpublished binary that runs scenarios on a headless GPU, so it needs no window and no operator. A scenario is a view and a closure that drives a `TestUi` one frame at a time and says whether the frame is warm-up, measured or the end. `mise run perf:bench [filter]` plays each scenario once for the exact numbers and then five more times under a `tracing` layer for the times, writes `target/bench/latest.json` and prints a row per scenario. A report maps each scenario to flat metrics: `frame.*` (`FrameStats`), `render.*` (`RenderStats`) and `memory.*` (`RenderMemory`) are read from the structs' `Debug` output, so a new counter shows up without touching the bench; `heap.*` counts the bench thread's allocations and bytes per measured frame and while building; `ms.*` are milliseconds: building the UI, CPU per frame (p50 and p99), the wait for the GPU to finish the frame, and each span per frame. A time is the fastest of the passes and its `.spread` is the slowest minus the fastest; the passes are interleaved across scenarios so that a busy moment lands in the spread. `mise run perf:bench --save` writes `target/bench/base.json`; later runs print what changed since then and fail when something got worse: any counter that differs, heap numbers that moved by more than 5%, and times that moved by more than 5% and more than either spread. `mise run perf:profile <scenario>` samples a scenario with the system profiler and prints its hottest functions; `mise run perf:heap <scenario>` writes a dhat heap profile to `target/bench/dhat-heap.json`.
- **Tests.** `rouge_core::testing::TestUi` (and `TestUiBuilder`: size, scale, fonts, palette, menus, input kind) runs a real `Ui` on a headless GPU with a deterministic clock (`advance`, `run`, `wait_until`) and drives it by test id (`click_on`, `double_click_on`, `hover_on`, `drag_on`, `tap_on`, `tap`, `swipe`, `swipe_over` for touches, `scroll_on`, `scroll_into_view`, `drop_files_on`, `press("cmd-k enter")`, `type_text`), with pixel reads and `render()` to an image. `rouge_render::headless::Headless` renders scenes to textures with PNG export. Tests use public APIs only and prefer integration tests in each crate's `tests/`.
- **TodoMVC.** `mise run run todomvc` (add `web` or `ios` to run on that target) is the cross-framework comparison app: a `KeyedList` model with memos for the remaining count and the visible rows, a `select` memo per row field, the filter as a memo over `route()`, `persisted` todos, a strikethrough span on completed titles and row enter and exit transitions. `tests/examples_test.rs` drives it by test id and asserts that toggling one todo costs the same effects, memos, layer paints and layouts with 50 rows as with 500.

## 18. Performance budgets

On an M-series Mac or a mid-range Vulkan GPU at 120Hz:

| Scenario | Target |
|---|---|
| Idle | 0 frames, 0% CPU |
| Compositor-only animation frame | under 2ms CPU |
| Single signal change to paint | under 1ms CPU, excluding GPU |
| Scrolling a 100,000-row virtual list | no dropped frames |
| 1,000 simultaneously animating layers | no dropped frames |
| Full-window glass panel over scrolling content | no dropped frames |
| Cold start to first frame | under 100ms after window creation |
| A surface presents a frame, any size | 0 bytes of pixel data through CPU memory; the cost of a compositor-only frame |
| Full-window animating surface under glass | no dropped frames |

How the design meets them:

- No per-frame tree rebuild or diff; work is proportional to change, and first values are computed at build.
- Dirty levels keep most updates out of layout; scrolling and compositor animation stay out of paint.
- Retained draw lists in one instance buffer; a clean layer costs a draw call and a few bytes of uniform records.
- Animations interpolate in the compositor over flat per-layer values.
- Text is shaped once per content and style, and re-broken only for new widths.
- Pass breaks (backdrops, rasterized layers) are merged, cached and counted.

## 19. Risks and open questions

| Risk or question | Position |
|---|---|
| **Run-once footguns**: reading a signal in a component body captures a stale value. | The main ergonomic cost of the model. Planned devtools warnings; clear docs. |
| **The replica goes stale**: a layer write that is not marked as touched never reaches the compositor. | Every layer write goes through `LayerModel`, which marks the layer, and the `TestUi` and `Stage` suites render through the replica, so a missed mark shows up as a wrong pixel or command. |
| **A blocked UI thread stalls animations** on the inline transport. | The native host composites on its own thread by default, which keeps drawing; `ROUGE_COMPOSITOR=inline` opts out. |
| **Backdrop pass breaks** can make frames expensive with many overlapping glass elements. | Merging, caching, reduced-resolution blur, and counters in `RenderStats`. |
| **CPU-rasterized paths** are resampled under compositor scale, and a canvas drawing new paths every frame uploads new masks. | Fine for icons and simple shapes; a GPU path renderer could back the same primitive later. |
| **Text quality** versus CoreText at small sizes. | Grayscale swash output; tuning in the glyph shader; a native rasterizer could slot in behind `TextSystem`. |
| **Gamma-space blending** differs from linear-light compositing. | Chosen for parity with CSS and design tools. |
| **winit limitations**: popups and layer-shell on Wayland, native menus off macOS. | Popovers render in-window; Linux menus are rendered by Rouge. |
| **Cross-API synchronization**: wgpu cannot see a foreign API writing a buffer it samples. | Read-only buffers from submit until the pool hands them out again, releases after GPU completion. Fences are deferred until the first foreign-API producer; they can be added to `Frame` without changing the rest of the protocol. |
| **Linux GL-to-Vulkan sharing** varies by driver (DMA-BUF export, modifiers, NVIDIA). | Opaque-fd sharing is the contingency; a GPU blit into a shared image is the last resort. A CPU copy is not on the list. |

## 20. Planned: web view (`rouge-webview`)

An optional crate embedding [Servo](https://servo.org) as a surface producer. Nothing else will depend on it, and it does not apply to the browser target.

**Scope.** Content the app author controls (bundled HTML, rendered documents, embedded tools); arbitrary sites are best-effort. Fully composited as a surface, with zero CPU copies per frame.

**Approach.**

| Option | Verdict |
|---|---|
| Servo with a software context or `read_to_image` | Rejected: CPU readback and re-upload on every repaint. |
| Blit Servo's framebuffer into a shared texture | Contingency only: a GPU copy that a custom context avoids. |
| **Servo with a custom `RenderingContext` that renders into the shared buffer** | **Chosen.** |
| System web view as a child window | Rejected: cannot be composited. |
| CEF offscreen rendering with shared textures | A possible second producer if open-web compatibility is ever required. |

**Rendering path.** A `SharedRenderingContext` owns the web view's `SurfaceProducer`, configured with three native-shared buffers, each attached to a GL framebuffer once. `prepare_for_rendering` acquires a free buffer and binds its framebuffer, and WebRender draws straight into shared memory; `present` submits the buffer, gated on the GL fence (this is the producer that brings back the deferred fences of section 12.6); `Configure` reallocates the chain at the new size. A page repaint is one WebRender draw and one compositor sample, with no copy on the CPU or GPU.

**Threading.** One Servo instance lives on a dedicated web thread, an actor with commands in over a channel, frames out through each producer, and state out through `signal.sender()`. Page animations are paced by frame requests, so hidden or occluded pages stop, and a static page produces no frames.

**Input.** Events arrive through normal dispatch in surface-local coordinates and are forwarded to Servo. Because Servo answers whether it consumed an event only after a round trip, frames will carry input regions (areas that consume wheel events, and cursor zones) so Rouge can decide scroll chaining synchronously from the current frame. Focus maps to `set_focused`, keys go to the page first, and the page's cursor drives the platform cursor.

**Constraints.** Pin an exact Servo release and gate upgrades on the crate's tests; build in its own CI job; Servo is MPL-2.0 while Rouge stays MIT; build without Servo's `webgpu` feature.

**Known and open.** On macOS, Servo drawing through a custom `RenderingContext` into an IOSurface imported into wgpu without copies, fence-gated triple buffering without stale or torn frames, and Servo running off the main thread are known to work. Open: DMA-BUF export from GL on Mesa and NVIDIA, forcing Servo's GL context onto the wgpu adapter's GPU on Linux, Servo's paint budget for heavy pages, how much of Servo's scroll tree is exposed for input regions, and custom-scheme resource loading.

## 21. Not built yet

- Accessibility beyond the desktop adapter. `rouge-core` builds an AccessKit tree from element semantics (`rouge_core::access`; `Ui::accessibility_tree`, `accessibility_update` and `accessibility_action`), `TestUi` queries it by role and name, and the native host connects it through `accesskit_winit` (`platform/native/runner.rs`): the adapter delivers requests to the target window through the winit event loop proxy, the first tree is sent when an assistive technology asks, and each step publishes what changed. The widgets declare roles, names and states, an element with `on_activate` is a button unless it says otherwise, and a control with no label of its own is named by the text inside it; `tests/gallery_test.rs` fails when an interactive element in the gallery has no role or name. The [plain-text editor](#accessible-plain-text-editing) has native macOS range checks; Linux/Orca has not been verified. Not built: the browser ARIA bridge, Android accessibility, accessible canvas text and rich document navigation.
- Devtools (`rouge-devtools`): inspector, invalidation flashing, layer view with pass breaks and surface sources, frame HUD, signal graph, warnings for per-frame layout animation and untracked reads in view code, hot reload as a stretch goal.
- `rouge-webview` (section 20). The compositor thread is the native default; its input-to-present latency and macOS live resize are not yet measured against the inline transport.
- Surfaces: Linux native sharing (DMA-BUF), fences (CPU-signalled, then native), frame damage and colour-space metadata, input regions, video sources (YUV, timed queues), CEF across processes, browser video through `copyExternalImageToTexture`.
- Partial present: frames redraw fully from retained buffers.
- Compositor: clip animation, non-uniform scale, `Motion` beyond `f32`, effect values in keyframes or bound to motions, backdrop effects inside rasterized layers.
- Text and images: colour glyphs (emoji), atlas eviction, image mipmaps, SVG documents, password masking.
- Browser: flushing pending `persisted` writes on `pagehide`, system font discovery and on-demand fallback fonts, EditContext, off-thread image decoding, timers for timed surface frames, multi-touch and pen pressure, media paste, an `OffscreenCanvas` worker.
- `combobox` and a calendar-grid form of `date_picker`, which wait on popover collision handling and flipping. `date_picker` is wheels only, with English month names.
- Touch: multi-touch, a sheet scaling the page behind it (the page is outside the overlay root; `Motion::map` is the building block), pull to refresh on content shorter than its viewport (the rubber band needs room to scroll), and hosts following a change of input kind after mount (a tablet gaining a mouse).
- Mobile text input beyond the first slice: on iOS the system selection display and edit menu, soft-keyboard composition verified on hardware, and linking the active accessibility node to the text responder for VoiceOver editing; on Android, predictive Back, and TalkBack editing verified on hardware. Neither host scrolls a focused field out from under the keyboard: `keyboard_inset()` moves docked chrome only.
- Mobile chrome: a `nav_bar` that floats as glass over its scroller with a title that follows the scroll offset (it is in flow, and its large title folds in one layout transition), the previous screen's title beside the back button, a lifted preview of the pressed element under a long-press `context_menu` (it opens as the same sheet), an edge swipe that opens a drawer, selection mode on `list_row` and `data_table`, fading edges on `chip_row` and `wheel`, and Escape reaching a `nav_stack` that neither holds focus nor lies on the root's first-child path.
- Shader loops are not phase-locked across elements: each effect's clock starts when it is applied, so two skeletons do not share frames the way repeating keyframes do.
- Rounded clipping of nested layers: the hold-to-confirm fill is clipped by a scissor rectangle, so it is square at the button's rounded corners.
- Popover collision handling and flipping; keyboard navigation of the rendered menu bar; an anchor-relative transform origin (overlays scale about their own centre).
- Reduced motion: hosts reporting the OS setting into `MotionScale`, and applying a change of scale to components that are already built.
- Disclosure height: a collapsing panel fades while the rows below slide with a layout transition; animating the clip itself needs clip animation or a size transition that starts from zero.
- A "next wake time" from `Ui`: a blinking caret requests a frame every vsync for ten seconds after editor activity.
- Benchmarks in CI: the counters of `mise run perf:bench` against a checked-in report once they are confirmed equal on Linux, and timings on a GPU runner.
- Benchmarks in a real window (frame pacing, late frames, time to first frame) and in the browser, and GPU time per render pass from timestamp queries.

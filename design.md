# Rouge: High-Level Design

A signal-driven, GPU-composited UI framework for Rust on macOS and Linux.

Status: draft v1, partially implemented. See [section 21](#21-implementation-status) for what exists and where the code differs from this document. "Rouge" is a working name taken from the workspace directory.

---

## 1. Summary

Rouge is a general-purpose native UI toolkit built on four ideas:

1. **Fine-grained signals.** Component functions run once. Every reactive prop is its own small effect that patches exactly one property of one retained node. There is no re-render, no diffing, and no manual caching.
2. **A declarative builder API with no macros.** gpui-style chains (`div().flex().gap(8.).child(...)`), laid out by taffy.
3. **One renderer on wgpu.** No per-OS drawing code. A custom instanced SDF renderer feeds a layer compositor that we fully control.
4. **A real compositor.** Layers, backdrop effects (liquid glass), custom WGSL effects, and animations that tick inside the compositor without touching the reactive graph, layout, or paint.

The performance contract: a signal change costs work proportional to what actually changed, a compositor animation frame costs almost nothing on the CPU, and an idle app produces zero frames.

## 2. Goals and non-goals

### Goals

- Signals with automatic dependency tracking (Svelte 5 / Solid model).
- High-level builder API, plain Rust functions as components, no proc macros required.
- macOS and Linux (Wayland and X11) from one codebase, with all drawing through wgpu.
- Compositing and effects as first-class features: blur, backdrop blur, glass/refraction, shadows, masks, blend modes, user-supplied WGSL.
- 120Hz target: 8.3ms frame budget, under 2ms CPU for compositor-only animation frames.
- Fully idle when nothing changes.
- v1 ships a core widget set and dev tooling.

### Non-goals for v1

- Windows, web, and mobile. Nothing in the design blocks them, but they are not targeted.
- Accessibility (AccessKit) and multi-window / native chrome. Both are designed for (section 17) and scheduled after v1.
- A software renderer or guaranteed GL fallback. Metal (through wgpu) on macOS and Vulkan on Linux are required; GL is best-effort.
- A dedicated render thread. The UI/compositor boundary is shaped so one can be added later (section 9.5).
- Native-looking platform widgets. Rouge draws everything itself.
- System web views (WKWebView, WebKitGTK) as child windows. They cannot be composited: no glass over them, no overlays above them, no compositor transforms. Embedded web content goes through surfaces (section 9.6) and Servo (section 15).
- Open-web compatibility. The web view targets content the app author controls. Arbitrary third-party sites are best-effort and bounded by Servo.
- Any per-frame CPU copy of surface pixels, including as a fallback (section 9.6).

### A note on "avoiding Metal"

wgpu on macOS runs on Metal underneath. What we avoid is writing or maintaining any Metal-specific code: all shaders are WGSL and all GPU calls go through wgpu.

There is one deliberate exception: importing externally produced GPU memory (an IOSurface on macOS, a DMA-BUF on Linux) as a wgpu texture for surfaces. That code is a few hundred lines per OS, lives in a single module of `rouge-render`, creates textures only, and never issues draw calls.

## 3. Decisions at a glance

| Area | Decision |
|---|---|
| Scope | General-purpose toolkit |
| Reactivity | Fine-grained, run-once components, no diffing |
| Signal model | `Copy` handles into a scope-owned arena; graph lives on the UI thread |
| API | Plain functions returning `impl View`; tailwind-style typed utility chains |
| Layout | taffy (flexbox, grid, block) |
| Renderer | Custom wgpu renderer: instanced SDF primitives, glyph atlas |
| Compositor | Retained layer tree, on the UI thread in v1 |
| Animation | Compositor-driven by default; signal-driven fallback for layout properties |
| Effects | Built-in set plus an `Effect` trait for custom WGSL |
| Text | Parley (layout, shaping, fallback) with swash/skrifa rasterization |
| Foreign content | Surfaces: externally produced GPU textures composited as layers, shared zero-copy, never read back |
| Web view | Servo, rendering straight into a shared GPU buffer, in an optional crate |
| Platform | winit behind an internal `Platform` trait, plus small per-OS shims |
| Perf target | 120Hz, modern GPUs, no software fallback |
| v1 extras | Core widget set, dev tooling |

## 4. Architecture

### 4.1 Crates

| Crate | Responsibility |
|---|---|
| `rouge` | Facade and prelude. What apps depend on. |
| `rouge-reactive` | Signals, memos, effects, scopes, batching. No UI dependencies; usable standalone. |
| `rouge-core` | Element tree, styles, taffy integration, events, focus, view builders, control flow. |
| `rouge-text` | Parley integration, glyph rasterization, text measurement, editing primitives. |
| `rouge-render` | wgpu device management, primitive pipelines, atlases, layer compositor, effects, animation engine. |
| `rouge-platform` | `Platform` trait, winit implementation, clipboard, cursor, IME, frame scheduling. |
| `rouge-widgets` | Button, inputs, scroll view, virtual list, popover, etc. Built only on the public API. |
| `rouge-devtools` | Inspector overlay, frame HUD, signal graph view, hot reload support. |
| `rouge-webview` | Servo embedding as a surface source. Optional; nothing else depends on it. |

### 4.2 Data flow

```
 signal.set()
     |
     v
 reactive graph  --(binding effects run)-->  element tree (retained)
                                                 |  dirty flags per node:
                                                 |  LAYOUT / PAINT / COMPOSITE
                                                 v
                          +----------- frame (only if something is dirty) -----------+
                          |  1. flush effects                                        |
                          |  2. layout     (taffy, dirty subtrees only)              |
                          |  3. paint      (rebuild draw lists of dirty layers only) |
                          |  4. commit     (layer property + draw list deltas)       |
                          |  5. composite  (tick animations, draw layers, effects)   |
                          |  6. present                                              |
                          +----------------------------------------------------------+
```

The key property is that each stage is skipped when its inputs are clean:

| Change | Reactive | Layout | Paint | Composite |
|---|---|---|---|---|
| Compositor animation tick (transform, opacity, effect uniform) | no | no | no | yes |
| Scroll | no (unless virtualized range changes) | no | no | yes |
| Surface presents a new frame (web view repaint, video frame) | no | no | no | yes |
| Background color signal changes | one effect | no | one layer | yes |
| Text content signal changes | one effect | subtree | one layer | yes |
| Nothing | no | no | no | no frame |

## 5. Reactivity (`rouge-reactive`)

### 5.1 Primitives

```rust
let count = signal(0);                          // Signal<i32>, Copy
let double = memo(move || count.get() * 2);     // Memo<i32>, Copy, lazy + cached
effect(move || println!("{}", double.get()));   // re-runs when double changes

count.set(1);
count.update(|c| *c += 1);
count.with(|c| ...);          // borrow without cloning
count.get_untracked();        // read without subscribing
batch(|| { a.set(1); b.set(2); });  // one flush
```

- `Signal<T>`, `Memo<T>`, `ReadSignal<T>`, and `WriteSignal<T>` are `Copy` generational ids into an arena. No `clone()` when moving into closures.
- Dependency tracking is automatic and dynamic: dependencies are re-collected on each run of a memo or effect.
- `T: 'static`. `get()` requires `T: Clone`; `with()` does not.

### 5.2 Propagation algorithm

Push-pull with graph coloring (the approach used by Reactively and Leptos):

1. A write marks direct dependents *dirty* and transitive dependents *check*.
2. Effects reachable from the write are queued.
3. On flush, each queued effect pulls its sources; a memo recomputes only if a source actually changed value (`PartialEq` short-circuit).

This is glitch-free (no effect sees an inconsistent state) and each node runs at most once per flush.

### 5.3 Scheduling

- Inside a batch, writes do not run effects; they flush when the outermost batch ends. Event handlers run inside a batch, so a handler's writes flush together.
- Outside a batch, a write flushes immediately after it. Writes made by an effect or memo while it runs are deferred until it finishes.
- Either way, effects have always run before the next layout.
- A write during flush is queued into the same flush, with a loop limit that panics in debug builds and reports the cycle.

### 5.4 Ownership and disposal

- Every signal, memo, and effect is owned by the **scope** that was current when it was created.
- Scopes form a tree mirroring the UI structure: app root, component, each branch of `show`, each row of `each`.
- Disposing a scope frees its nodes, runs `on_cleanup` callbacks, and removes its elements.
- Using a disposed handle panics in debug with the creation location (`#[track_caller]`), and `try_get` / `try_set` are available. In release it is a no-op for writes and a panic for reads.
- State that must outlive the place it was created is created in an ancestor scope and passed down, or provided through context.

### 5.5 Threading and async

- The graph is single-threaded and lives on the UI thread. Handles are `!Send`.
- The graph is owned by an explicit `Runtime`. `runtime.enter()` returns a guard that makes it current on the calling thread; signals, effects and scopes panic when none is entered. Dropping the runtime disposes everything it owns. `App` creates one and enters it for the duration of `run()`; headless users of `Ui` create and enter their own.
- `spawn(async { ... })` runs a future on the UI-thread executor, so it can touch signals directly between awaits.
- `spawn_blocking` / `background(async { ... })` run on a thread pool and return a future for the result.
- `resource(move || id.get(), |id| async move { fetch(id).await })` gives an async-derived signal with loading/ready/error state, re-fetching when its source changes and cancelling stale requests.
- `signal.sender()` returns a `Send` handle whose `set` posts to the UI thread, for integration with foreign threads and channels.

### 5.6 Context

`provide(value)` and `use_context::<T>()` look up the scope tree. Used for theme, focus manager, window handle, and app services.

## 6. View API (`rouge-core`)

### 6.1 Example

```rust
use rouge::prelude::*;

fn counter() -> impl View {
    let count = signal(0);
    let theme = use_theme();

    div()
        .flex()
        .col()
        .gap(12.)
        .p(16.)
        .rounded(12.)
        .backdrop(Glass::new().blur(24.).refraction(0.4))
        .child(
            text(move || format!("Count: {}", count.get()))
                .size(18.)
                .color(theme.text),
        )
        .child(
            button("Increment")
                .on_click(move |_| count.update(|c| *c += 1)),
        )
        .child(show(
            move || count.get() > 5,
            || text("That's a lot").color(Color::RED),
        ))
}

fn main() {
    App::new().window(WindowOptions::default(), counter).run();
}
```

### 6.2 Components

- A component is a function that returns `impl View`. It runs exactly once, inside its own scope.
- Props are plain function arguments. Components with many optional props use a builder struct that implements `View`.
- Reactive props are passed as signals or closures: `fn avatar(user: Signal<User>, size: impl IntoProp<f32>)`.
- `AnyView` provides type erasure for heterogeneous children and recursive components.

### 6.3 Reactive props

Every style and content method takes `impl IntoProp<T>`, implemented for:

- a plain `T` (static: written once, no effect created),
- `Signal<T>`, `Memo<T>`, `ReadSignal<T>`,
- closures `Fn() -> T`.

A reactive prop creates one **binding effect** that writes the new value into the node and sets the narrowest dirty flag for that property (section 7.2). Static props cost nothing at runtime beyond the initial write.

Coherence for the closure blanket impl alongside value impls is a known Rust friction point. If it cannot be expressed cleanly for all `T`, the fallback is a marker-type parameter (`IntoProp<T, M>`), which is invisible at call sites.

### 6.4 Styling

- Typed tailwind-style methods: `.flex()`, `.grid()`, `.w(px(200.))`, `.w_full()`, `.px(4.)`, `.gap(8.)`, `.rounded(8.)`, `.bg(color)`, `.border(1., color)`, `.shadow(Shadow::md())`, `.opacity(0.8)`.
- State variants take a closure over a style refinement: `.hover(|s| s.bg(theme.hover))`, `.active(...)`, `.focus(...)`, `.focus_visible(...)`, `.disabled(...)`.
- `Style` is a first-class value: build it, store it, merge it with `.style(my_style)`. Widgets expose named style slots this way.
- Theme is a struct of signals provided through context, so a theme switch updates only the bound properties.
- There is no cascade and no selector engine.

### 6.5 Control flow

Because components run once, structure changes go through reactive helpers, each managing child scopes:

- `show(cond, || view)` and `show_else(cond, || a, || b)`: mount and dispose a branch.
- `each(items, |item| key, |item| view)`: keyed list. Reconciliation is by key over the *data*, so rows are moved, created, or disposed; surviving rows are untouched. Each row receives its item as a signal so in-place updates stay fine-grained.
- `switch(value, |v| match v { ... })`: re-mounts when the discriminant changes.
- `dynamic(move || -> AnyView)`: escape hatch that re-runs a closure and replaces its subtree.

Plain Rust `if` and `for` work for structure that is static after construction.

### 6.6 Element set

`div`, `text`, `rich_text`, `image`, `svg`, `canvas` (custom draw callback with a painter API), `scroll`, `virtual_list`, `surface` (externally produced GPU content, section 9.6), and `overlay` (portal to the window's overlay root for popovers, menus, and tooltips).

`webview` is not in the core set. It is built on `surface` and ships in `rouge-webview` (section 15).

## 7. Element tree, layout, and invalidation

### 7.1 Retained tree

- Elements live in a slotmap arena; each has a taffy node, a resolved style, event handlers, optional text layout, and a reference to the layer it paints into.
- The tree is mutated only by binding effects and control-flow helpers. Nothing is rebuilt per frame.

### 7.2 Dirty classes

Each style property is statically classified, and a write sets the narrowest flag:

| Flag | Properties | Consequence |
|---|---|---|
| `LAYOUT` | size, padding, margin, gap, flex/grid properties, text content, font | taffy relayout of the affected subtree, then repaint |
| `PAINT` | background, border, corner radius, text color, shadow | rebuild the draw list of the owning layer |
| `COMPOSITE` | transform, opacity, clip, effect parameters, scroll offset | update a layer property; no layout, no paint |

### 7.3 Layout

- taffy drives flexbox, grid, and block layout. Text and images are measured leaves (`measure` functions backed by Parley and intrinsic image size, with measurement caching).
- Taffy's own dirty tracking plus cached layouts confines relayout to affected subtrees.
- Layout results (bounds per element) are stored in window-space-independent form: positions relative to the owning layer, so moving a layer does not invalidate its contents.
- Transforms never affect layout.

### 7.4 Scrolling

- A `scroll` element owns a layer; its scroll offset is a compositor property. Scrolling changes no element, runs no effect, and repaints nothing.
- Scroll offset is observable as a signal for code that wants it (sticky headers, virtualization), updated at most once per frame.
- `virtual_list` derives its visible range from the offset with a memo, so rows mount and dispose only when the range actually changes, with overscan to keep that off the critical path.
- Momentum and rubber-banding run in the compositor's animation engine.

## 8. Rendering (`rouge-render`)

### 8.1 Primitives

All primitives are instanced quads with analytic (SDF) coverage in the fragment shader, in the style of gpui's renderer:

| Primitive | Notes |
|---|---|
| Quad | Rounded rect with per-corner radii, solid / linear / radial gradient fill, border, optional squircle (continuous corner) shape |
| Shadow | Analytic blurred rounded-rect (no offscreen pass) for drop and inner shadows |
| Glyph run | Alpha-mask atlas for text, color atlas for emoji |
| Image | Sampled quad with corner clipping; decode and upload off the UI thread |
| Path | Tessellated with lyon into a mesh; cached per path + scale. MSAA or coverage-based AA for edges |
| Underline / strikethrough | Including wavy |
| Layer | A texture or sub-layer drawn with transform, opacity, and optional effect |
| Surface | An external texture sampled in place with rounded-rect clipping; shares the sampled-quad pipeline with Image |

### 8.2 Draw lists

- Painting a layer produces a **draw list**: primitives sorted by z-order and batched by pipeline into GPU instance buffers.
- Draw lists are retained per layer. A layer whose content is clean is not repainted; its existing buffers are reused as-is.
- Clipping uses scissor rects where axis-aligned and SDF clip (rounded-rect clip in the fragment shader) for rounded content masks, avoiding stencil in the common case.

### 8.3 Atlases and resources

- Glyph atlas keyed by (font, glyph id, size, subpixel offset bucket); shelf-packed, multiple pages, LRU eviction.
- Image cache with size-aware eviction; mip generation for downscaled images.
- Texture pool for offscreen targets (layer caches, blur chains), reused across frames.

### 8.4 Color and blending

- Colors are specified in sRGB; the public `Color` type supports sRGB, Display P3, and OKLCH constructors.
- Compositing blends in gamma (sRGB-encoded) space by default so results match CSS and design tools. Effects that need linear light (blur, glass) convert inside their shaders.
- Wide gamut (P3) surface where available on macOS.

### 8.5 Frame pacing

- Frames are produced only when a dirty flag is set or an animation is active.
- Presentation is vsynced (`Fifo`), driven by the platform's redraw / frame callback. The animation clock samples the predicted presentation time, not wall-clock at tick time.
- Variable refresh displays run at up to 120Hz during animation and input, and drop to zero frames when idle.

## 9. Compositor, layers, and effects

### 9.1 Layer tree

A **layer** is the unit of compositing: a retained draw list plus compositor properties (transform, opacity, clip, scroll offset, effect chain, blend mode).

An element gets its own layer when it:

- has a compositor animation or transition declared on it,
- has an effect, backdrop effect, mask, non-normal blend mode, or group opacity,
- is a scroll container, or
- is explicitly marked with `.layer()`.

Everything else paints into its nearest ancestor layer. Promotion is automatic and stable (declaring a transition promotes the element up front rather than at animation start, so there is no hitch on the first frame).

### 9.2 Direct replay vs. cached textures

Layers have two composition modes, chosen automatically:

- **Direct**: the layer's retained instance buffers are drawn straight into the parent target with the layer's transform and opacity applied as per-layer uniforms. This is the default. With SDF primitives it stays crisp under scale and costs no texture memory.
- **Rasterized**: the layer is rendered to an offscreen texture, then drawn as a single quad. Used only when required: group opacity over overlapping content, any content effect (blur, color matrix, custom shader), masks, or when `.layer().rasterize()` is requested for very expensive static content. The texture is reused until the layer's content is dirty.

This keeps texture memory and overdraw low while still making compositor-only frames cheap: such a frame updates uniforms and re-issues draw calls for existing buffers.

### 9.3 Backdrop effects

A backdrop effect reads what has already been drawn behind an element.

1. The scene renders into an offscreen scene target rather than directly into the swapchain.
2. Layers draw back to front. On reaching a layer with a backdrop effect, the current pass ends.
3. The region under the layer (inflated by the effect's sampling margin) is copied and processed into a backdrop texture. Blur uses a dual-Kawase downsample/upsample chain.
4. A new pass begins and the layer's backdrop shader draws, sampling the backdrop texture, followed by the layer's own content.

Cost controls:

- Backdrop layers at the same depth that do not overlap share one pass break and one blur chain.
- If nothing behind a backdrop layer changed this frame, its processed backdrop texture is reused.
- Blur runs at reduced resolution proportional to radius.
- Dev tooling shows the pass-break count per frame, since this is the main way to make a frame expensive.

### 9.4 Effects API

Built-ins, applied as style methods:

```rust
div().effect(Blur::new(8.))                    // content effects
     .effect(ColorMatrix::saturate(1.4))
     .backdrop(BackdropBlur::new(20.))         // backdrop effects
     .backdrop(Glass::new().blur(24.).refraction(0.4).dispersion(0.05).tint(c))
     .mask(Mask::linear_gradient(...))
     .blend(BlendMode::Screen)
```

`Glass` is implemented as a backdrop shader that uses the element's rounded-rect SDF to derive an edge normal and thickness profile, offsets backdrop UVs by a refraction term, optionally splits channels for dispersion, then adds tint, a specular rim highlight, and an inner shadow. All parameters are animatable uniforms.

Custom effects implement a trait:

```rust
struct Ripple { frequency: f32, strength: f32 }

impl Effect for Ripple {
    const WGSL: &'static str = include_str!("ripple.wgsl");
    const INPUTS: Inputs = Inputs::CONTENT;            // or BACKDROP, or BOTH
    fn uniforms(&self) -> Vec<f32> { vec![self.frequency, self.strength] }
    fn margin(&self) -> f32 { self.strength.abs() }    // extra area drawn and sampled
}
```

The WGSL defines one function, `fn effect(in: EffectInput) -> vec4<f32>`, returning a premultiplied color. A Rouge prelude is prepended to it and provides:

- `in.local` (logical pixels from the element's top-left), `in.size`, `in.uv`, `in.distance` (signed distance to the element's rounded rect) and `in.coverage` (its antialiased coverage),
- `sample_content(local)` and `sample_backdrop(local)`, which take positions in the same local space and follow the layer's transform,
- `shape_sdf(local)`, `scale_factor()`, and `param(i)`, which returns the i-th `vec4` of the effect's uniforms.

An effect has up to 16 uniform floats, in the order `uniforms()` returns them. `content_blur()` and `backdrop_blur()` ask for an input to be blurred before the shader samples it, which is how `Blur`, `BackdropBlur` and `Glass` are built. Effect values accept reactive props (`.effect(move || Blur::new(radius.get()))`); when the bound value changes on an element with a transition, every uniform, the blur radii and the margin animate in the compositor as a `COMPOSITE` change.

### 9.5 Commit boundary

The UI side never touches the compositor's state directly. Each frame it emits a **commit**: a list of layer creations, removals, property changes, new draw lists, and animation commands. The compositor applies the commit, ticks animations, and draws.

In v1 both sides run on the UI thread. Keeping the boundary as a data-only commit means the compositor can move to a dedicated render thread later (keeping animations smooth while the UI thread is busy) without changing the public API.

### 9.6 Surfaces

A **surface** is a layer whose content is a GPU texture produced outside Rouge's painter: a web view, a video decoder, a 3D viewport, a camera feed. It is the single way foreign pixels enter the scene. The web view (section 15) is its first consumer and is not special-cased anywhere in the compositor.

**The rule.** The frame path never moves surface pixels through CPU memory. There is no readback (`glReadPixels`, `read_to_image`) and no `queue.write_texture` per frame, and there is no software fallback that does either. A producer that cannot share GPU memory with the compositor's device fails when it attaches with a typed `SurfaceError`, and the element mounts its fallback view. A 1600x1200 web view at 2x is 30.7 MB per frame; copying that out and back at 120Hz is 7.4 GB/s of memory traffic for no visual result, which is why this is a rule and not a tuning choice.

**Because a surface is an ordinary layer**, everything in sections 9.1 to 9.5 and section 10 applies without new code paths: transforms, opacity, rounded clipping, scroll containers, enter/exit, layout transitions, glass and backdrop blur over it, overlays above it, and hit testing through animated transforms.

#### Element and producer API

```rust
surface(source)
    .w_full()
    .h(px(400.))
    .rounded(12.)
    .fallback(|error| text(error.to_string()))
```

```rust
pub trait SurfaceSource: 'static {
    fn attach(&mut self, cx: SurfaceCx) -> Result<(), SurfaceError>;
    fn configure(&mut self, config: SurfaceConfig);
    fn detach(&mut self);
}
```

- `surface` is a taffy leaf that always owns a layer. Layout decides its size; it has no intrinsic size.
- `SurfaceCx` carries the app's wgpu `Device` and `Queue`, the adapter's identity (so a producer can check it is on the same GPU), the import functions below, and a `SurfacePresenter`.
- `SurfaceConfig` is the physical pixel size, the scale factor, and visibility. Rouge sends it when layout, scale, or visibility changes. Visibility is false when the layer is fully clipped, its window is occluded, or it is only alive for an exit animation.
- `SurfacePresenter` is `Send`. `present(SurfaceFrame)` posts the frame to the UI thread over a channel and wakes the event loop. Producers are actors: they own their resources, run on whatever thread suits them, and talk to Rouge only through `configure` and the presenter. Nothing is shared behind a lock.
- `SurfaceFrame` is data only: a `wgpu::Texture` handle, the valid rectangle inside it, the row origin (`TopLeft` or `BottomLeft`), the alpha mode (`Opaque` or `Premultiplied`), and a release token.

#### Compositing

- A presented frame becomes `LayerCommand::Surface(SurfaceFrame)` in the next commit. It is a `COMPOSITE` change: no effect runs, no layout, no paint, and no draw list is rebuilt.
- The renderer samples the texture in place with the sampled-quad pipeline, clipped by the layer's rounded-rect SDF. Bind groups are cached per texture, so a steady-state present allocates nothing.
- Row origin is handled by flipping UVs in the vertex stage. GL producers are bottom-left; there is no flip pass.
- `Opaque` frames draw without blending.
- Surface pixels are sRGB-encoded, matching the gamma-space scene target (section 8.4), so no conversion pass is needed.
- A surface layer is never rasterized into a cache texture: it already is one. A surface inside a rasterized ancestor, or behind a backdrop effect, invalidates that cache on every present; devtools reports this.
- During a resize the compositor stretches the last frame to the new bounds until a frame at the new size arrives, so a live resize never shows a blank rectangle.
- When a surface is removed with an exit animation, the layer keeps its last frame. The producer is already detached.

#### Buffer ownership and synchronization

- The producer owns a small swap chain (three buffers by default) of shared GPU buffers. Each buffer is imported into wgpu **once**, when the chain is allocated or resized, not per frame.
- A presented buffer is read-only for the producer until its release token comes back. The compositor returns the token over a channel after the frame has been replaced and the last GPU submission that sampled it has completed (`Queue::on_submitted_work_done`). This matters because wgpu cannot see a foreign API writing into a texture it is still sampling.
- A producer publishes a frame only after its own rendering into the buffer is complete on the GPU: it creates a fence after its last draw call and presents when the fence signals. It never calls `glFinish`, and the compositor never waits on a producer. A late producer frame is shown one compositor frame later; it cannot stall a compositor frame.
- At most one frame per surface is pending. A newer present replaces an unconsumed older one, whose token is returned immediately.

#### Importing shared memory

Both import paths are verified to exist in wgpu 30.0.1, the version the workspace pins.

| OS | Shared object | Import |
|---|---|---|
| macOS | IOSurface | `newTextureWithDescriptor:iosurface:plane:` on the `MTLDevice` taken from `Device::as_hal`, wrapped with `wgpu_hal::metal::Device::texture_from_raw` and `Device::create_texture_from_hal` |
| Linux | DMA-BUF | `wgpu_hal::vulkan::Device::texture_from_dmabuf_fd` under `Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF` (`VK_EXT_external_memory_dma_buf`, `VK_EXT_image_drm_format_modifier`), then `create_texture_from_hal` |

- `SurfaceCx::import_io_surface` and `SurfaceCx::import_dmabuf` wrap these. They are the only OS-specific GPU code in the workspace.
- DMA-BUF is chosen on Linux over opaque-fd sharing (`VK_KHR_external_memory_fd` plus `GL_EXT_memory_object_fd`) for three reasons: it is a plain wgpu feature flag where opaque fds need hal-level device creation; it is what VA-API video decoders and Wayland already speak, so the same path serves video; and it is not tied to both sides using the same driver build. Opaque fd is the named contingency if the S0 spike finds DMA-BUF export unusable on a required driver.
- `Gpu::new` requests the DMA-BUF feature when the adapter offers it. When it is absent, `import_dmabuf` returns `SurfaceError::Unsupported`.
- A producer on a different GPU from the compositor (dual-GPU laptops) gets `SurfaceError::AdapterMismatch`. Cross-GPU sharing is out of scope.

#### One device per app

Imported textures belong to one wgpu device, and a producer must be able to outlive a window. The app therefore owns a single `Gpu` shared by all windows. Today each window creates its own (section 21); M7 changes that, which is also what multi-window needs (section 17).

#### Testing

- A reusable `TestSurface` source in the crate's test utilities renders a known pattern into a real wgpu texture and presents it. Pixel tests cover clipping, origin flip, alpha modes, opacity, transforms, glass over a surface, resize stretch, and exit snapshots through the headless renderer.
- `RenderStats` gains a count of surface bytes written through the queue. It must stay zero; tests and the benchmark assert it.

## 10. Animation

### 10.1 Compositor-driven (default)

Animatable without layout or paint: translate, scale, rotate, opacity, clip, scroll offset, corner radius and background color of a layer-backed element, and any effect uniform.

Three ways to use it:

```rust
// 1. Transitions: animate whenever the bound value changes.
div().opacity(move || if open.get() { 1.0 } else { 0.0 })
     .scale(move || if open.get() { 1.0 } else { 0.95 })
     .transition(Transition::all(Spring::snappy()))

// 2. Motion values: imperative, compositor-owned, not signals.
let x = motion(0.0);
div().translate_x(x)
     .on_click(move |_| x.animate_to(200.0, Spring::bouncy()));

// 3. Keyframes: declarative, looping or one-shot.
div().animate(Keyframes::new().at(0.0, |s| s.rotate(deg(0.))).at(1.0, |s| s.rotate(deg(360.)))
        .duration(ms(800)).repeat())
```

- With a transition, the binding effect runs once when the signal changes and hands the compositor a target value. Per-frame interpolation then happens entirely inside the compositor.
- Springs are velocity-preserving on retarget, so interrupted animations stay continuous.
- `Motion<T>` supports `set`, `animate_to`, `stop`, `get()` (sample the current value), and completion futures (`x.animate_to(...).await`). It can be driven directly by gestures for drag and swipe.
- Timing: springs, cubic-bezier easing, duration tweens, decay (momentum).

### 10.2 Signal-driven (fallback)

Layout-affecting properties (width, height, padding, gap) can be animated by a `tween` or `spring` signal source that writes each frame. This relayouts the affected subtree per frame. It is correct and bounded, but it is the slow path, and dev tooling flags it.

### 10.3 Layout transitions

`.layout_transition(Spring::smooth())` gives FLIP-style animation: when layout changes an element's bounds, layout runs once to the final state, and the compositor animates a transform (and optionally size via scale or clip) from the old rect to the new one. This covers reordering, expand/collapse, and shared-element moves at compositor cost.

### 10.4 Enter and exit

`show` and `each` accept `.enter(...)` and `.exit(...)` transitions. On exit, the scope is disposed immediately for reactivity purposes, but its layer is kept alive as a non-interactive snapshot until the exit animation completes.

## 11. Text (`rouge-text`)

- **Parley** for layout: shaping, bidi, line breaking, font fallback and system font discovery (fontique), rich text spans, inline boxes.
- **swash / skrifa** to rasterize glyphs into the atlas, with subpixel positioning buckets on the x axis. Grayscale AA by default.
- `text` is a taffy measured leaf. Parley layouts are cached per (content, style, width constraint).
- A text content change is `LAYOUT` dirty; a color change is `PAINT` only.
- Editing: built on Parley's editor primitives for cursor movement, selection, and IME composition, wrapped by `rouge-widgets` text input and text area.
- Large documents (code-editor scale) are out of scope for the built-in text area; `canvas` plus `rouge-text` line layout is the supported route for building one, with a dedicated editor widget as a later phase.

Text stack details (the exact Parley feature set and shaper version) should be pinned against the crate's current release at implementation time.

## 12. Platform (`rouge-platform`)

- An internal `Platform` trait covers: window lifecycle, surface handles, input events, IME, cursor, clipboard, scale factor, frame scheduling, and posting to the UI thread.
- v1 implementation is **winit** on macOS, Wayland, and X11.
- Small per-OS shims fill gaps where needed (for example macOS titlebar and window-background configuration). These stay behind the trait.
- Later phases add native menus (muda), file dialogs (rfd), drag-and-drop, and tray behind the same trait.
- The trait exists so that winit can be replaced on a single platform if frame pacing or popup handling requires it, without touching the rest of the framework.

## 13. Events, input, and focus

- Hit testing walks the layer tree front to back using current (animated) transforms and clips, then the element tree within the hit layer. Because the compositor is on the UI thread in v1, hit testing always sees the on-screen position.
- Dispatch has capture and bubble phases. Handlers are builder methods: `.on_click`, `.on_pointer_down/move/up`, `.on_scroll`, `.on_key_down`, `.on_focus`, `.on_blur`; `event.stop_propagation()` is supported.
- Hover, active, and focus state are tracked by the framework and drive the style variants without user signals.
- Pointer capture for drags; a gesture layer (drag, pan with velocity) feeds `Motion` values directly.
- Focus: a focus tree with tab order, focus scopes for modals and popovers, and `focus_visible` distinction for keyboard vs. pointer.
- Keyboard shortcuts: typed actions bound to key chords in a context-sensitive keymap (the gpui model), dispatched along the focus path.
- Elements carry an optional semantic role and label from day one so accessibility can be added without API churn.

## 14. Widgets (`rouge-widgets`, v1)

Built strictly on the public API, which keeps that API honest.

- Button, toggle, checkbox, radio, slider
- Text input, text area
- Scroll view, virtual list
- Popover, tooltip, context menu, dropdown/select
- Modal / dialog
- Tabs, separator, progress, spinner

Widgets are minimally styled, read colors and metrics from the theme, and expose named `Style` slots for overriding. In v1, popovers and menus render in the window's overlay root rather than in separate OS windows.

## 15. Web view (`rouge-webview`)

An optional crate that embeds [Servo](https://servo.org) as a surface source (section 9.6). Scheduled after v0.1 (M8); nothing in the core crates depends on it.

### 15.1 Scope

- For content the app author controls: bundled HTML/CSS/JS, rendered documents, embedded tools. Servo is not WebKit or Chromium, and arbitrary sites will break.
- Fully composited. A web view is a surface layer, so glass, overlays, clipping, and animation all work over and on it.
- Zero CPU copies per frame, per the surface rule.

### 15.2 Why Servo, and why not the obvious shortcuts

| Option | Verdict |
|---|---|
| Servo with `SoftwareRenderingContext` or `read_to_image` (what `iced_servo` does) | Rejected. CPU readback and re-upload on every repaint. |
| Servo, blit its framebuffer into a shared texture (Slint's Linux path) | Rejected as the design; kept as a contingency. It is a GPU copy, not a CPU one, but it is avoidable: a custom `RenderingContext` can make Servo draw into the shared buffer directly. |
| **Servo with a custom `RenderingContext` that renders into the shared buffer** | **Chosen.** |
| System web view as a child window (wry) | Rejected. Cannot be composited; child views on Linux are X11-only. |
| CEF offscreen rendering with shared textures | Not now. A viable second `SurfaceSource` if open-web compatibility becomes a requirement; costs a ~200 MB bundle and a multi-process runtime. |
| Wait for a wgpu backend in WebRender ([servo#37149](https://github.com/servo/servo/issues/37149)) | Not a plan. No merged status as of November 2026. If it lands, Servo could render on Rouge's device and `rouge-webview` would shrink; the surface API would not change. |

### 15.3 Rendering path

Servo paints with WebRender over OpenGL, in a GL context managed by surfman. Its embedding API takes any implementation of the public `RenderingContext` trait, and calls `prepare_for_rendering` before drawing so the context can bind a framebuffer.

`rouge-webview` implements that trait as `SharedRenderingContext`:

1. It owns the surface swap chain: three shared buffers (IOSurface on macOS, DMA-BUF on Linux), each attached to a GL framebuffer and each imported once into wgpu through `SurfaceCx`.
2. `prepare_for_rendering` binds the framebuffer of the current back buffer. WebRender draws the page straight into shared memory.
3. `present` flushes, creates a fence, rotates the chain, and hands the finished buffer to the `SurfacePresenter` once the fence signals.
4. `resize` reallocates the chain at the new physical size.
5. `read_to_image` is implemented with a readback because the trait requires it. Servo uses it for screenshots only; it is never on the frame path.

One `SharedRenderingContext` per web view, because Servo sizes a web view to its rendering context.

End to end, a page repaint is: WebRender draws into buffer N, the fence signals, a `SurfaceFrame` is posted, one compositor frame samples buffer N. No copy on either the CPU or the GPU.

### 15.4 Threading and frame pacing

- One `Servo` instance per process, owned by a dedicated **web thread**, together with every `WebView` and `SharedRenderingContext`. Servo's handles are `!Send`, so they stay on the thread that created them. The web thread is an actor: commands in over a channel, frames out through `SurfacePresenter`, state out through `signal.sender()` (section 5.5).
- Servo's layout and script already run on Servo's own threads. What remains on the embedder thread is `spin_event_loop` and `WebView::paint` (WebRender's GL submission). Keeping those off the UI thread means a slow page cannot drop a Rouge animation frame.
- Servo's `EventLoopWaker` wakes the web thread. `WebViewDelegate::notify_new_frame_ready` triggers `WebView::paint` then `present`.
- Page animations (`notify_animating_changed`) are paced by Rouge's vsync through Servo's `RefreshDriver` hook, not a free-running timer, so pages animate at display rate and stop when the window is occluded.
- Idle is still idle: a static page produces no Servo frames, so no surface presents and no Rouge frames. A hidden web view (`SurfaceConfig` not visible) is hidden in Servo and stops painting.

If the S0 spike shows Servo cannot run off the main thread on a target OS, the fallback is to run the same actor on the UI thread behind the same channels. The public API does not change; the cost is that page paint time counts against the UI frame budget.

### 15.5 Input, focus, and cursor

- The web view element receives events through normal dispatch (section 13). Hit testing already maps through animated transforms, so events arrive in surface-local coordinates.
- Pointer, wheel, keyboard, and IME composition events are forwarded with `WebView::notify_input_event`. Servo reports whether the page consumed each one (`notify_input_event_handled`); unconsumed wheel and key events continue bubbling in Rouge, which gives scroll chaining and app-level shortcuts.
- Focus maps to `WebView::set_focused`. Tab traversal enters and leaves the page through the focus tree.
- `notify_cursor_changed` sets the platform cursor while the pointer is over the element.
- Scale factor changes go to `set_hidpi_scale_factor`.
- Keyboard, focus, and IME depend on M5. Until then only pointer and wheel can be forwarded.

### 15.6 API

```rust
let page = webview("app://index.html");

page.view()
    .w_full()
    .h_full()
    .rounded(12.);

page.load("app://settings.html");
page.title();
page.loading();
page.evaluate("document.title").await;
```

- `webview(url)` returns a `Copy` handle. `view()` builds the element; the other methods send commands to the web thread.
- `title()`, `url()`, `loading()`, and `can_go_back()` are read signals.
- Navigation policy is a callback (`on_navigate`) that allows or denies a request.
- App resources are meant to be served from a custom scheme, so bundled content needs no local server. Whether Servo's embedding API exposes protocol registration to embedders is to be confirmed in M8; `data:` and `file:` URLs are the baseline.
- Two-way messaging between page script and Rust is planned on top of `evaluate` and a message channel. Its shape is decided in M8, not here.

### 15.7 Dependencies and build

- **Servo version**: pinned to an exact release. Servo publishes 0.x releases roughly monthly (0.8.0 on 2026-11-07) and has had breaking changes between them. Upgrades are deliberate and gated on the M8 test suite.
- **Build cost**: Servo is a very large dependency with native components (SpiderMonkey). It is isolated in `rouge-webview` so the core workspace build and CI are unaffected, with its own CI job.
- **Licensing**: Servo is MPL-2.0. Rouge stays MIT OR Apache-2.0; apps that enable the web view take on MPL obligations for Servo's files. Documented in the crate README.
- **Servo features**: built without Servo's `webgpu` feature at first, so Servo does not pull a second wgpu into the build.
- **Interop code**: written in-house against wgpu's import APIs (section 9.6). The [Slint Servo example](https://slint.dev/blog/using-servo-with-slint) and [wgpu-graft](https://github.com/merely-made/wgpu-graft) are prior art for the same technique and are used as references. wgpu-graft is not taken as a dependency: it is young, single-maintainer, MPL-2.0, and the code we need is small.

### 15.8 What is established and what the spike must prove

| Claim | Status |
|---|---|
| Servo is embeddable from crates.io and accepts a custom `RenderingContext` | Established (Servo 0.8.0 docs). |
| wgpu 30 can wrap an IOSurface-backed Metal texture and a DMA-BUF without copying | Established (APIs read in the pinned wgpu-hal source). |
| Servo output can reach a wgpu texture on macOS with no copy | Established in principle: Slint and godot-servo do IOSurface to `MTLTexture`. Slint adds a flip pass, which UV flipping makes unnecessary. |
| WebRender renders correctly into a framebuffer bound in `prepare_for_rendering` | Expected: Servo's own `OffscreenRenderingContext` works this way. To prove with our buffers in S0. |
| A GL framebuffer backed by an exported DMA-BUF works on Mesa (Intel, AMD) and NVIDIA | Unproven. S0. |
| Fence-gated presentation is sufficient on both OSes with no tearing or stale frames | Unproven. S0. |
| Servo can be created and spun on a non-main thread on macOS and Linux | Unproven. S0. |
| Servo's GL context can be forced onto the same GPU as the wgpu adapter on Linux | Unproven; surfman's EGL backend has no GPU selection API. S0. |
| Servo paint time for representative pages at 2x | Unmeasured. S0 sets the M8 budget. |

## 16. Dev tooling (`rouge-devtools`, v1)

- **Inspector overlay**: pick an element to see its bounds, resolved style, layout box, owning layer, and creation location.
- **Invalidation flashing**: relayout, repaint, and composite highlighted in distinct colors.
- **Layer view**: layer boundaries, direct vs. rasterized mode, texture memory, pass-break count. Surface layers show their producer, buffer size, present rate, and any cache they invalidate.
- **Frame HUD**: per-stage timing (effects, layout, paint, composite, GPU), frame-time graph, dropped frames.
- **Signal graph**: live signals, memos, and effects per scope, with run counts and what triggered the last run.
- **Warnings**: per-frame layout animation, effect loops, signals read outside a tracking context in view code (a common run-once mistake), leaked scopes.
- **Hot reload**: stretch goal. Run-once components fit function-level hot patching (subsecond-style) well, since a patched component's scope can be disposed and re-run in place. Feasibility is validated in its own milestone and it does not block v1.

Devtools are compiled out unless the `devtools` feature is enabled.

## 17. Designed for, shipped later

- **Accessibility**: AccessKit tree generated from element roles and labels, updated incrementally from the same dirty flags. Parley provides the text hooks.
- **Multi-window and native chrome**: `App` owns a set of windows, each with its own root scope, element tree, and compositor sharing one wgpu device and one atlas set. v1 ships a single window per app plus transparent/custom titlebar support where winit provides it.
- **Render thread**: enabled by the commit boundary (section 9.5).
- **Partial present / damage rects**: v1 redraws the full frame from retained buffers when anything changed.
- **Windows and web**: wgpu and winit both support them; the work is in text, IME, and platform shims.
- **Surfaces and web view**: designed in sections 9.6 and 15, scheduled as M7 and M8 after v0.1. The S0 spike runs earlier because its findings constrain device creation.
- **More surface sources**: video (VA-API and VideoToolbox already produce DMA-BUFs and IOSurfaces), user-rendered wgpu content, and CEF if open-web compatibility is needed. All use the M7 API unchanged.

## 18. Performance plan

### Budgets (M-series Mac or mid-range discrete/integrated Vulkan GPU, 120Hz)

| Scenario | Target |
|---|---|
| Idle | 0 frames, 0% CPU |
| Compositor-only animation frame | under 2ms CPU |
| Single signal change to paint | under 1ms CPU excluding GPU |
| Scrolling a 100k-row virtual list | no dropped frames at 120Hz |
| 1,000 simultaneously animating layers | no dropped frames at 120Hz |
| Full-window glass panel over scrolling content | no dropped frames at 120Hz |
| Cold start to first frame | under 100ms after window creation |
| Surface present, any buffer size (M7) | 0 bytes of pixel data through CPU memory; Rouge-side cost of a compositor-only frame (under 2ms CPU) |
| Full-window animating surface under a glass panel (M7) | no dropped frames at 120Hz |
| Web view idle on a static page (M8) | 0 frames, 0% CPU |
| Web view scrolling a text-heavy page, full window at 2x (M8) | 120Hz target; the budget for Servo's own paint is set from S0 measurements |
| Rouge animation while a web view page is busy (M8) | no dropped Rouge frames |

### How the design meets them

- No per-frame tree rebuild or diff; work is proportional to change.
- Dirty classification keeps most updates out of layout.
- Retained draw lists and instance buffers; clean layers cost a draw call.
- Animations interpolate in the compositor over flat arrays of layer properties.
- Glyph and layout caches; image decode and upload off the UI thread.
- Pass breaks (backdrop effects, rasterized layers) are counted, batched, and cached.

### Verification

- Benchmark apps for each budget row, run in CI on a GPU runner where available and tracked over time.
- Headless rendering to texture for golden-image tests of primitives and effects.
- Unit and property tests for the reactive graph (glitch-freedom, disposal, run-at-most-once).
- Frame-stage timing built into the runtime behind a feature flag, feeding the HUD.

## 19. Roadmap

| Milestone | Deliverable | Exit criteria |
|---|---|---|
| **M0: Reactive core** | `rouge-reactive`: signals, memos, effects, scopes, batching, context | Test suite passes; micro-benchmarks comparable to existing Rust signal libraries |
| **M1: Pixels** | winit window, wgpu setup, quad + shadow pipelines, element tree, taffy, static `div` tree, reactive props | Counter demo without text; signal change repaints only the affected layer |
| **M2: Text and input** | Parley integration, glyph atlas, `text`, events, hit testing, hover/active/focus variants, control flow (`show`, `each`) | Todo-list demo; keyed list reorder touches only moved rows |
| **M3: Compositor** | Layer tree, direct vs. rasterized modes, commit boundary, scroll layers, transitions, springs, `Motion`, keyframes, layout transitions, enter/exit | 1,000-layer animation benchmark meets budget; idle is zero frames |
| **M4: Effects** | Scene target, backdrop pipeline, blur, `Glass`, masks, blend modes, `Effect` trait with custom WGSL | Liquid-glass demo over scrolling content meets budget |
| **M5: Widgets** | `rouge-widgets` v1 set, text editing with IME, focus traversal, keymap/actions, images, paths, `canvas`, async (`spawn`, `resource`) | A non-trivial sample app (e.g. a chat client or settings app) built only on public API |
| **M6: Devtools and hardening** | Inspector, HUD, signal graph, warnings, hot-reload spike, docs, Linux X11/Wayland pass, benchmark suite in CI | All budget rows met on both OSes; v0.1 release |
| **S0: Surface interop spike** | Time-boxed to two weeks, throwaway code, runs alongside M5. Servo with a custom `RenderingContext` drawing into an IOSurface (macOS) and a DMA-BUF (Linux), imported into wgpu and drawn by the existing renderer through a temporary hook. Resolves every "unproven" row in section 15.8. | Written go/no-go per OS with: zero CPU pixel copies confirmed by instrumentation; Servo paint times for three reference pages at 2x; the sync scheme validated for tearing and stale frames; driver matrix (Apple silicon, Intel/AMD Mesa, NVIDIA); threading answer. A no-go on DMA-BUF switches Linux to the opaque-fd contingency before M7 starts. |
| **M7: Surfaces** | App-wide `Gpu` shared by windows; sampled-quad pipeline (shared with M5 images); `surface` element; `SurfaceSource`, `SurfaceCx`, `SurfacePresenter`; `LayerCommand::Surface`; swap-chain release tokens; IOSurface and DMA-BUF import; `TestSurface`; devtools reporting. Depends on M5 images and M6's Linux pass. | Surface budget rows met on both OSes; pixel tests for clip, flip, alpha, transforms, glass over a surface, resize and exit; `RenderStats` surface upload bytes is zero in every test and benchmark; idle is zero frames. |
| **M8: Web view** | `rouge-webview`: `SharedRenderingContext`, web-thread actor, vsync-driven refresh, input/IME/focus/cursor forwarding, `webview` handle with signals, navigation policy, resource loading, script evaluation and messaging. Depends on M7 and M5 input. | Web view budget rows met on both OSes; a sample app with a web view under a glass toolbar, inside a scroll container, with a popover above it; text entry with IME in a page form; scroll chaining out of the page; pinned Servo version builds in its own CI job. |
| **Post-v1** | AccessKit, multi-window, native menus/dialogs, render thread, damage rects, editor-grade text widget, further surface sources (video, CEF) | |

M0 has no dependencies and M1's renderer work is independent of it until reactive props land, so the two can proceed in parallel.

M7 and M8 ship after v0.1. S0 is deliberately early: it is cheap, it depends only on the renderer that already exists, and a no-go changes how `Gpu` is created, which is better known before M6 hardens it. M7 has value without M8 (video and custom GPU content), so it is a separate milestone and not a web view implementation detail.

## 20. Risks and open questions

| Risk / question | Mitigation or current position |
|---|---|
| **Run-once footguns**: reading a signal in a component body (not in a closure) silently captures a stale value. | Debug-mode warning for untracked reads during view construction; devtools surfacing; clear docs. This is the main ergonomic cost of the model. |
| **`IntoProp` coherence** between plain values and closures. | Marker-type parameter fallback (section 6.3). Prototype in M1 before the style API is written out. |
| **Disposed-handle access** with `Copy` signals. | Generational ids, debug panics with creation site, `try_` variants. |
| **winit limitations**: frame pacing on macOS, popups/layer-shell on Wayland, native menus. | `Platform` trait allows per-OS replacement; popovers are in-window in v1. |
| **Backdrop pass breaks** can make frames expensive with many glass elements. | Batching, caching, reduced-resolution blur, and a visible counter in devtools. Establish a tested upper bound in M4. |
| **Text rendering quality** vs. native (CoreText) on macOS, especially at small sizes. | Evaluate swash output against native in M2; gamma/contrast tuning in the glyph shader; a native rasterizer behind `rouge-text` remains possible without API change. |
| **Gamma-space vs. linear blending** default. | Start with gamma-space for parity with design tools; revisit after M4 once effects exist. |
| **Path rendering** via tessellation may be insufficient for heavy vector content. | Acceptable for icons and simple shapes in v1; a compute-based path renderer could back the same primitive later. |
| **UI-thread compositor**: a blocked UI thread stalls animations. | Async and background helpers keep work off the UI thread; the commit boundary keeps the render-thread option open. |
| **Hot reload feasibility** with closures and generic views. | Time-boxed spike in M6; not a v1 commitment. |
| **GL-to-Vulkan sharing on Linux** varies by driver (DMA-BUF export, modifiers, NVIDIA). | S0 tests a driver matrix before M7 is committed. Opaque-fd sharing is the contingency; a GPU blit into a shared image is the last resort. A CPU copy is not on the list: unsupported configurations get `SurfaceError` and the fallback view. |
| **Cross-API synchronization**: wgpu cannot see GL writing to a buffer it samples. | Producer-owned swap chain, fence-gated present, release tokens returned after GPU completion (section 9.6). Validated in S0 with a tearing test. |
| **Servo API churn**: 0.x releases roughly monthly with breaking changes. | Exact version pin, upgrades gated on the M8 suite, all Servo types confined to `rouge-webview`. |
| **Servo web compatibility** is well short of WebKit and Chromium. | Scope is author-controlled content (section 15.1). `SurfaceSource` leaves room for a CEF source without API change. |
| **Servo paint cost** is unmeasured and could exceed a 120Hz frame on heavy pages. | Servo runs on its own thread, so it cannot stall Rouge frames; the page itself may update below display rate. S0 measures it and sets the M8 budget. |
| **Servo off the main thread** may not work on every OS. | S0 answers it. Fallback is the same actor on the UI thread (section 15.4). |
| **Servo build weight** (compile time, native toolchain, binary size). | Optional crate with its own CI job; never a dependency of the core crates. |
| **Dual-GPU machines**: Servo's GL context and the wgpu adapter may land on different GPUs. | Adapter identity is checked at attach; mismatch is a typed error. Forcing a matching GPU on Linux is an S0 question. |
| **Name**: "Rouge" is a placeholder. | Check crates.io availability before publishing. |

## 21. Implementation status

Milestones M0 to M5 are implemented, and the S0 spike has been run on macOS. Everything listed as done is covered by tests, including pixel-level tests that render on a real GPU. Development and all windowed runs have been on macOS (Apple silicon); see "Linux" below for what has been run there.

### Done

| Area | What exists |
|---|---|
| Reactivity (`rouge-reactive`) | Explicit `Runtime` with an enter guard; signals, memos, effects, scopes, batching, context, untrack, cleanup; push-pull propagation; disposal with creation-site diagnostics. Async: `spawn` (a UI-thread executor owned by the runtime, tasks owned by the scope or effect that spawned them), `spawn_blocking`, `background`, `sleep`, `resource`, and `Signal::sender` for writes from other threads. `Runtime::poll` / `poll_tasks` drive it and `Runtime::set_waker` tells the embedder when there is work. |
| Renderer (`rouge-render`) | wgpu pipelines for quads (rounded, bordered), shadows, glyphs and sampled images (`ImageQuad`: RGBA or alpha-mask `ImageData`, source rect, tint, rounded-rect clip; textures uploaded once, cached per image and evicted when idle or over budget; a draw list is drawn as one call per run of same-texture primitives); glyph atlas; retained per-list GPU buffers; one effect pipeline per `Effect` type, built from a shared WGSL prelude; region-limited dual-Kawase blur at reduced resolution; backdrop snapshots that are batched and cached; headless rendering with PNG export. |
| Effects (`rouge-render`) | The `Effect` trait for custom WGSL, and the built-ins written against it: `Blur`, `ColorMatrix`, `Mask` (linear, radial, shape), `BlendMode` (twelve modes), `BackdropBlur`, `Glass`. |
| Compositor (`rouge-render`) | Retained layer tree driven only by data-only `Commit`s; per-layer transform, opacity, fill color, corner radii, scroll offset and clip; content effect chains, mask, blend mode and backdrop effect per layer, with animatable parameters; direct replay and rasterized (cached texture) layers chosen automatically; the animation engine (springs, tweens with cubic-bezier easing, decay, keyframes, motion values, layout transitions with optional size, enter/exit); scroll momentum and rubber-banding; scene generation for the renderer. |
| Text (`rouge-text`) | Parley layout (wrapping, alignment, weights, families, composable byte-range spans), swash rasterization with subpixel positioning. Editing primitives over Parley's cursor and selection: carets and selections from byte indices or points, movement by character, word, line and document, selection rectangles and caret geometry. |
| Core (`rouge-core`) | Retained element tree, taffy layout with text measurement, `Styled` builder API with reactive props, dirty classes, `div`, `text` and `rich_text`, hover/active variants, pointer down/move/up and scroll-wheel events with bubbling, pointer capture, a drag gesture with velocity, a `window_size` signal, `show` / `each` / `dynamic` with `.enter()` / `.exit()`, layers with segmented paint order and layer-relative draw lists, transitions, `motion`, `.animate(Keyframes)`, `.layout_transition()`, `scroll` and `virtual_list`, `.effect()` / `.backdrop()` / `.mask()` / `.blend()`. M5 additions: keyboard events with bubbling, a focus tree (tab order, `tab_index`, focus scopes, autofocus with restore, `FocusHandle`), `.focus()` / `.focus_visible()` / `.disabled()` variants and `.enabled()`, typed actions bound to key chords along the focus path, IME events, cursors, hover enter/leave, pointer events with element-local coordinates, modifiers and click count, `.pass_through()`, `.track_bounds()`, `overlay`, a `Theme` of signals in context, `editable` (single- and multi-line text editing with selection, clipboard, IME composition), `image` (decoded off the UI thread, four fit modes), `Path`, `canvas` with a `Painter`, and `icon`. |
| Widgets (`rouge-widgets`) | Built only on the public API: button, toggle, checkbox, radio, slider, text input, text area, scroll view, popover, tooltip, context menu, select, modal, tabs, separator, progress, spinner; `virtual_list` is re-exported from the core. Each reads the theme and implements `Styled`, so style methods override its defaults. |
| App (`rouge`) | winit event loop, surface management, vsynced redraw only when dirty or animating, trackpad scroll phases, keyboard and modifier mapping, IME enablement and cursor area, cursor icons, the system clipboard (arboard), event-loop wake-ups for async work, examples (`counter`, `glass`, `effects`, `motion`, `diff`, `settings`, `snapshot`, `bench_layers`, `bench_scroll`, `bench_glass`). `settings` is the M5 sample app: tabs, forms, theme switching, a popover, a modal, a context menu, a key binding and an async resource, all through `rouge::prelude`. |

### Measured

Release mode, 800x600 at 2x scale on an Apple silicon laptop. CPU time unless stated; `bench_glass` also reports the time until the GPU has finished the frame, with no vsync.

- `bench_layers`, 1,000 layers springing at once: 0.55ms average per frame (1.6 to 1.9ms worst across runs), zero instance bytes uploaded, zero layout or paint passes.
- `bench_scroll`, a 100,000-row `virtual_list` scrolled 40px per frame: 0.11ms average per frame (1.6ms worst) with about 150 elements mounted. Frames that stay inside the overscan run no layout and no paint; crossing it re-lays-out and repaints only the list's window layer.
- `bench_glass`, a `Glass` panel with a 24px blur covering the window over that scrolling list: 0.19ms CPU and 0.8ms including the GPU per frame, with one pass break. Four stacked panels: 0.44ms and 2.1ms. Twelve panels with an 8px blur: 0.89ms and 3.6ms. Stacked panels touch, so each needs its own pass break; twelve is the largest count tested.
- Re-run after M5 (the draw list now draws one call per run of same-texture primitives): `bench_layers` 0.57 to 0.59ms average (1.9 to 2.1ms worst); `bench_scroll` 0.11 to 0.16ms average, with a worst frame of 1.5 to 1.9ms in three runs and an unexplained 15 to 16ms frame in two others; `bench_glass` with one panel 0.16ms CPU and 0.59ms including the GPU.
- Idle: `Ui::needs_frame` is false whenever nothing is dirty and nothing is animating, and the event loop requests no redraw.

### Differences from this document

- **Rem lengths**: `Length::Rem` and `rem` work for every Length-taking layout property and text size. A per-window `root_font_size() -> Signal<f32>` defaults to 16; root-size changes re-resolve retained rem declarations and invalidate their layout. Plain numeric text sizes remain pixels. Text-size fractions resolve against the root size; `auto()` uses the 14px text default.
- **Keymap queries**: `Ui::binding_for::<T>()` and `use_binding::<T>() -> Memo<Option<Chord>>` use the same focus path, chord shadowing and deepest action handler as key dispatch. Queries do not execute arbitrary key-down handlers to predict whether they would stop propagation. `Ui::dispatch_action(value)` invokes the deepest typed handler without requiring a key binding. `Chord::parse` accepts `cmd`, `command` and `secondary` as platform-command aliases; `display` produces platform labels.
- **Hover groups and test IDs**: `.group(name)` on a `Div` names an ancestor group; `.group_hover(name, |style| ...)` is a `Styled` variant driven by that ancestor’s existing hover state. `Div::test_id` accepts owned or borrowed names. `Ui::bounds_of` returns the first match in tree paint order and `bounds_of_all` returns all matches, in window coordinates; hidden subtrees are excluded. `TestUi` adds fallible `center_of`, `click_on`, `hover_on` and `drag_on`. Queries traverse the live retained tree, including newly rebuilt `dynamic` subtrees.
- **Inherited text styles**: `Styled` exposes `text_color`, `text_size`, `font`, `font_weight`, `italic` and `line_height` on any styled element. A leaf resolves each field independently from the nearest declaration, then its scoped theme (color and size) or the text defaults. Existing `Text::color`, `weight` and `bold` use these same declarations. Wrapping, alignment and spans remain leaf-local. Updates compare resolved leaf values: color changes repaint only inheriting layers without a layout pass, while typography clears only affected leaf layouts. Explicit overrides shield a leaf from ancestor changes. Line height remains a font-size multiple, not a Length.
- **Rich text**: `rich_text(content, spans)` returns the existing `Text` leaf, also exposing `.spans(prop)`. `TextStyle::spans` is shaped through the same Parley layout as plain text. Overlapping spans compose per field, with later values winning; backgrounds paint before glyphs and underline/strike rectangles after them. `SpanStyle` uses optional color, background, weight, italic, underline (`Underline { thickness, color }`), strikethrough and family. Range hover/click callbacks and `Ui::text_index_at` report the UTF-8 byte at the start of the grapheme under the pointer, not a nearest insertion caret; outside text returns `None`.

- **Flush timing**: see section 5.3, which was updated to match.
- **Prop conversions** use a dedicated `PropFrom` trait plus the marker parameter. A blanket `Into` bound made bare float literals ambiguous for `f32` props. Effects use their own `IntoEffect` conversion for the same reason.
- **No `rouge-platform` crate yet** (M5 added keyboard, IME, cursor and clipboard to the same place): winit is used directly from `rouge`. The `Ui` type is the platform-independent boundary and is what tests drive.
- **One `Gpu` per window**: each window creates its own wgpu instance and device. Surfaces need a single app-wide device (section 9.6); M7 makes that change.
- **Commit boundary**: `Commit`, `Compositor` and the animation types live in `rouge-render`. The element tree only ever pushes commands. Two things read back across the boundary, both on the UI thread: hit testing asks the compositor for a layer's current pose, and once per frame `Ui` copies motion values and scroll state out for `Motion::get` and `scroll_state`.
- **Compositor-animatable properties** are translate, scale (uniform), rotate, opacity, scroll offset, the background color and corner radii of an element that owns a layer, and effect parameters. Clip is not. Background and radius are layer properties read by the shader, so animating them uploads no instance data; they also work in `Pose`, so keyframes and enter/exit can use them. An element's border and text are still painted.
- **`Motion` is `f32` only** (not `Motion<T>`). `animate_to` and `decay` return an `Animation` that can be ignored or awaited; awaiting it yields `true` on arrival and `false` when interrupted. `get` returns the value as of the last frame.
- **Timing** is springs, tweens (four cubic easings or `Ease::cubic_bezier`), and `Decay` for motion values.
- **Scrolling**: wheel deltas scroll by exactly what the OS delivers and clamp. Trackpad gestures that report phases go through `Ui::scroll_drag` and `Ui::scroll_release`: past a limit the content follows with resistance, and on release it springs back. `scroll_release` with a velocity decays in the compositor and bounces off the limits; nothing in the app loop produces a non-zero velocity yet, since the OS already supplies momentum as further deltas. An axis with nothing to scroll does not rubber-band.
- **Keyframes** take poses over the compositor properties, a duration in seconds and one easing applied per segment. One-shot keyframes hold their final pose. Effect parameters are not available in poses.
- **Layout transitions animate position**, measured relative to the parent layer, and size when built with `LayoutTransition::new(timing).size()`. Size is animated by stretching the layer, so its content is scaled while the transition runs.
- **Enter and exit** take a `Pose` and a timing: `.enter(Pose::new().opacity(0.0), Spring::snappy())`. Rows present at first mount enter too. An exiting layer is drawn above the rest of its parent layer's content until it finishes.
- **Rasterized layers** are rendered in window space into a texture sized to their content, so they stay crisp but are re-rendered when anything other than their opacity or effect parameters changes. A layer is rasterized when it has a content effect, mask or blend mode, when it is translucent and holds more than one primitive, or when `.rasterize()` is set. A translucent layer with a backdrop effect somewhere below it is not rasterized and falls back to per-primitive opacity.
- **Effect chains**: each content effect, the mask and the blend mode is its own offscreen stage, applied in that order. Blend modes are an effect that reads both the element and the backdrop, so a blended element costs a backdrop snapshot.
- **Effect uniforms** are a flat list of up to 16 floats read with `param(i)`, not a user-defined struct, and there is no `#[derive(Uniforms)]`. Parameters animate through the element's transition when a bound effect value changes; they cannot be bound to a `Motion`.
- **`ColorMatrix` is 3x4**: it maps RGB with an offset and leaves alpha alone.
- **One backdrop effect per element.** Backdrop effects, and blend modes, only see the window's scene: inside a rasterized layer or a layer with content effects, a backdrop effect is skipped and a blend mode draws normally.
- **Clipping across layers uses scissor rects**: exact for axis-aligned content, the bounding box of the clip under rotation, and never rounded. `.mask(Mask::shape())` clips a layer to its rounded shape at the cost of rasterizing it.
- **Pointer capture and gestures**: `event.capture()` in a pointer-down handler routes moves and the release to that element until the button comes up. `.on_drag()` captures implicitly once the pointer has moved 3px, reports start, move and end with translation and velocity, and replaces the click. Drag is the only gesture. Every element under the pointer takes part in hit testing, so an overlay such as a glass panel blocks the wheel from reaching what is behind it.
- **`scroll()` returns a `Div`** whose children live in an implicit content layer. It scrolls on both axes and has no scrollbars. Attach a `Signal<ScrollState>` with `.scroll_state(state)` and call `state.update(|s| s.scroll_to(offset))` for programmatic scrolling. A scroller at its limit passes the wheel to its nearest scrollable ancestor.
- **`virtual_list`** accepts uniform heights or an indexed height closure. Indexed heights have per-row memos: changing a height dependency re-evaluates only affected closures, recomputes prefix positions, and preserves the partially visible top row and its intra-row offset. `VirtualListHandle` is `Copy`, created by `virtual_list_handle()` and attached with `.handle(h)`; it provides `scroll_to_row(index, Align::{Top, Center, Bottom, Visible})`, `scroll_to(offset)`, `scroll_state()`, `top_row()` (first fully visible row, via the frame-published scroll state), `row_top(index)` and `row_bounds(index)` for mounted rows. Row bounds and `Ui::bounds_at` are in window coordinates, including compositor transforms. Scroll requests clamp at the ends; an empty list reports top row zero.
- **Layer origins and scroll offsets snap to device pixels** so text stays sharp; animated translations do not.
- **Backdrop cost controls**: a backdrop effect copies only the region under the element, grown by the blur's reach, and blurs it at half or quarter resolution. Consecutive backdrops share one pass break when nothing drawn since the break touches the later one's region; each still runs its own blur. A backdrop is reused without a pass break when everything drawn before it is unchanged from the previous frame. Blur radii are continuous, but the kernel changes shape at each power of two, so an animated radius is smooth rather than exact.

### M5: differences from this document

- **Paths are rasterized on the CPU, not tessellated.** A filled or stroked `Path` is rendered by zeno into an alpha mask at device scale and drawn through the image pipeline with a tint; masks are cached per path, stroke, scale and subpixel offset. lyon is not used. Edges are antialiased without MSAA, but a path under a compositor scale animation is resampled like text, and a canvas that draws new paths every frame uploads a new mask each time. This is the "could back the same primitive later" case from section 20 taken in the other direction, and should be revisited if heavy vector content matters.
- **`svg` is `icon(Path::svg(d))`**: SVG path data in a view box, filled or stroked in one colour. There is no SVG document support.
- **`canvas` takes `Fn(&mut Painter)`** and runs inside an effect, so it redraws when a signal it reads changes, or when its size does. The painter has rects/quads, path fill and stroke, images and unwrapped text.
- **Images**: PNG and JPEG through the `image` crate, decoded with `spawn_blocking`. No mip generation, so heavily downscaled images alias. Uploading an image is one `write_texture`; that is separate from the surface rule in section 9.6.
- **Focus**: the `focus` variant applies to the focused element only (no focus-within); a wrapper that needs it reads `FocusHandle::is_focused`, which is how `text_input` draws its border. When nothing is focused, key events go to the window's root view. A click on something not focusable clears focus.
- **Disabled** is a state set with `.enabled(prop)`; `.disabled(|s| ...)` is its style variant. A disabled element gets no pointer handlers, focus, hover or active styles, and neither do its descendants, but ancestors still see the events.
- **Actions**: `.bind_key("cmd-s", Save)` and `.on_action(|save: &Save| ...)` live on elements; the deepest binding on the focus path wins and the deepest handler for that action type runs. There is no separate keymap object or global table. `cmd` means the platform command key.
- **Text editing** is an `editable(signal)` element in the core, wrapped by the widgets, not Parley's `PlainEditor`. Not built: undo/redo, caret blink, password masking, scrolling a multi-line field to keep the caret visible (a text area grows with its content).
- **Async**: `spawn_blocking` and `background` start one OS thread per call; there is no pool. `resource` re-fetches whenever its source closure re-runs, so wrap the source in a `memo` to dedupe. `Runtime::set_waker` may be called once. `Runtime::poll` runs until no task is ready, so a task that always wakes itself would spin it.
- **Popovers and menus** render in the overlay root and take a full-window catcher while open, so an outside click dismisses and is swallowed. Placement is fixed (below, above, left, right); there is no flipping or collision handling. Tooltips are plain text.
- **Scroll view** shows a vertical thumb that tracks the offset in the compositor; it cannot be dragged, and there is no horizontal thumb.
- **Slider and tabs** are laid out by signals, so dragging a slider relayouts its own subtree each frame.
- **taffy keeps a stale list of out-of-flow children** on a node that loses its last child while holding an absolutely positioned one. The overlay root keeps a hidden placeholder child for that reason.

### S0 spike

Run on macOS (Apple M4 Pro); the code and the full findings are in `spikes/s0-surface-interop/` (`FINDINGS.md`). It is throwaway and not part of the workspace.

- **macOS: go.** An IOSurface was imported into wgpu 30.0.1 with no copy, a GL producer and then Servo 0.8.0 (through a custom `RenderingContext`) drew straight into it, a fence-gated three-buffer swap chain showed no stale or torn frame in 2,300 fully checked frames at 3200x2400, UV flipping handled the bottom-left origin, and Servo ran on a non-main thread. `WebView::paint` took 0.7 to 1.9 ms at p50 on three synthetic pages at 2x, with 2 to 6 ms of GPU time behind the fence.
- **Linux: no verdict.** Nothing in S0 was run there: no DMA-BUF export or import, no driver matrix, no EGL GPU selection. The opaque-fd decision is still open and M7 on Linux should not start before it.
- Corrections to sections 9.6 and 15 that M7 and M8 need (the `objc2-metal` feature required for the IOSurface import, the exact `texture_from_raw` and `create_texture_from_hal` signatures, Servo needing both `gleam` and `glow` contexts, `LoadStatus::Complete` not meaning the new page is what paints next) are listed in the findings.
- On this machine `glFlush` alone, without a fence, also produced no stale frames, so the fence is shown to be sufficient but not shown to be necessary.

### Linux

- The full M5 workspace builds and all its tests pass in an aarch64 Linux container with Mesa llvmpipe (software Vulkan), run in parallel.
- Before M5, parallel test runs there crashed inside `libvulkan` while naming debug objects on concurrently created devices (`view_test` in 9 of 10 runs). `Headless` now creates its instance without `InstanceFlags::DEBUG`; one full parallel run since then was clean.
- Building needs `pkg-config` and `libfontconfig1-dev` (fontique); tests need a Vulkan driver. Cross-checking from macOS stops at the fontconfig build script.
- Not run at all: x86_64, a real GPU, and anything with a window (X11 or Wayland).

### Not yet built

- Compositor leftovers: clip animation, non-uniform scale, `Motion<T>` beyond `f32`, effect parameters in keyframes and bound to motions, backdrop effects inside rasterized layers, hit-test pass-through for overlays.
- M5 leftovers: SVG documents, undo/redo and caret blink in text editing, draggable scrollbars, popover collision handling, image mips, a thread pool behind `spawn_blocking`. Typing, IME and clipboard have been exercised through `Ui` in tests but not by hand in a window.
- M6: devtools, hot reload, benchmark suite in CI.
- S0 on Linux, M7, M8: surfaces and the web view. The sampled-quad pipeline M7 needs now exists (images), but there is no `surface` element and no `rouge-webview` crate.
- Text gaps: color glyphs (emoji), atlas eviction.
- Linux beyond what is listed above.

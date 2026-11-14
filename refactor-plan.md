# Rouge Refactor Plan

Goal: fewer things the user must know, fewer transformations between a state change and a pixel, and fewer places where the same fact is stored twice. Rouge should feel like a small toolkit with powerful internals, not powerful internals the application author has to help operate.

This plan combines two independent reviews of the working tree as of 2026-11-14, including the uncommitted refactor (about 150 changed files). Nothing has been benchmarked. Performance claims are source-level findings and hypotheses until the Phase 1 benchmarks confirm them.

Provenance tags on each item:

- **[verified]**: confirmed by reading the cited code during review.
- **[reported]**: found by a reviewer reading the code but not independently re-checked. Confirm it before starting the item.

## Constraints

These hold for every item below.

- **Protected decisions in `design.md` §3.2 stay**: the browser target and WebGL2 limits, the custom browser host, `view!` and `#[component]`, and the standalone `rouge-theme`, `rouge-syntax`, `rouge-markdown` and `rouge-reactive` crates.
- **Keep** run-once components, the retained compositor, one shared `Driver`, one parse path per document crate, paged uniform records, and the `FrameStats`/`RenderStats` counters.
- **No immediate-mode rewrite.**
- **`AGENTS.md` rules apply**:
  - one way to do something (replace, never add a parallel path);
  - `thiserror` for errors;
  - no new locking;
  - no comments;
  - integration tests in `tests/`, mirroring source paths;
  - builder-pattern fixtures;
  - real objects or crate fakes, not mocks;
  - `?` instead of `unwrap` in tests.
- **Update `design.md` in the same change** as any behaviour or API it describes.
- **Line-count honesty.** The concrete items remove roughly 3–4k of about 33k source lines (10–12%). Halving the codebase would require dropping features (backdrop effects, surfaces, the web host, incremental documents). That is out of scope here and is a separate decision. Moving code behind feature flags or into another crate is not a cut.

## Phase overview

| Phase | Theme | Depends on | Rough size |
|---|---|---|---|
| 0 | Fix-now bugs and waste | none | small, independent PRs |
| 1 | Baseline benchmarks and counters | none (run alongside 0) | small |
| 2 | User-facing API consistency | none; 2.1 before 2.2 | medium |
| 3 | Core internals | Phase 1; 3.1 before 3.2 | large |
| 4 | Frame representation (compositor → renderer) | Phase 1, 3.3 | large, prototype first |
| 5 | Per-crate cuts | none; batch with nearby work | medium |
| 6 | Docs and examples | runs alongside every phase | small |

Recommended order: Phase 0, then Phase 1 → 2.1 → 2.2 → 2.3 → 3.1 → 3.3 → 3.2 → 2.5 → 4 → 3.4/3.5. Phase 5 items land whenever the file is already being touched.

---

## Phase 0: Fix now

Each item is an independent PR. No API change unless noted.

### 0.1 Nested group textures are evicted while their parent's cache is fresh [verified]

- **Where:** `crates/rouge-render/src/renderer.rs:1818`. `self.groups.retain(|_, group| group.used == frame)` runs every frame. `used` is set only in `plan_group` (line ~1427), and a group's children are planned only when the group itself is stale (lines ~1398–1408). So while an outer group's cache is fresh, its inner groups are dropped. The compositor wraps every effect stage as a nested group (`crates/rouge-core/src/compositor.rs` ~980–995). A group that is briefly at opacity 0 or offscreen is also evicted at lines ~1380–1385.
- **Change:** use the same idle-window policy as images and instance slots: keep a group while `frame - used < IDLE_FRAMES`. That leaves one eviction policy instead of two.
- **Test:** in `crates/rouge-render/tests/renderer_test.rs`, a layer with two content effects. Change only the outer stage's input, render N frames, and assert through `RenderStats` that the inner stage is not re-rasterized. Add the same check for a group faded to 0 and back.

### 0.2 Wake on a deadline instead of rendering every frame for the caret [verified]

- **Where:** `crates/rouge-core/src/tree/edit.rs:323` (`caret_animating` is true for 10 s after activity) and `crates/rouge-core/src/ui.rs:327` (`needs_frame`). The caret only changes visibility every 0.5 s, but the host renders continuously.
- **Change:** replace `needs_frame() -> bool` with `next_frame() -> Wake`:

  ```rust
  pub enum Wake { Idle, At(f64), Now }
  ```

  - The compositor animating, a pending queue or a dirty tree → `Now`.
  - The caret → `At(next_flip)`.
  - The native host maps `At` to `ControlFlow::WaitUntil`.
  - The web host maps `At` to `setTimeout`, then `requestAnimationFrame`.
  - `Driver::wants_frame` follows the same change.
- **Consumers:** `crates/rouge/src/driver.rs`, `platform/native/runner.rs`, `platform/web/host.rs`, `rouge-core/src/testing.rs` (`TestUi`).
- **Test:** `crates/rouge-core/tests/ui_test.rs`. Focus an editor, type, advance the test clock by 0.1 s, and assert `At(..)` not `Now`. Assert `Idle` after 10 s.
- This is the first piece of the frame contract in 3.3.

### 0.3 Effect pipeline lookup hashes the WGSL source on every draw [verified]

- **Where:** `renderer.rs:978` (`effect_index: FxHashMap<&'static str, u32>`), lookup at line ~1210, called per effect draw (~1634). Glass is several KB of source.
- **Change:** use `Vec<(&'static str, RenderPipeline)>` with a `ptr::eq` scan (`fingerprint` already keys on the address, ~line 179). Delete `effect_index`.

### 0.4 Group caches miss on every frame of a translate or opacity animation [reported]

- **Where:** group freshness hashes the device-space affine (`renderer.rs` ~192) and compares `area == cover` (~1396). A translucent, multi-primitive layer that fades and slides re-renders its offscreen pass every frame.
- **Change:** rasterize groups in group-local space and apply `group.transform` once at composite time. The effect path at ~1430–1435 already does this.
- **Test:** a translucent multi-primitive layer with a translate transition. Assert zero group re-rasters after the first frame of the animation.

### 0.5 Tree-sitter is fed 64-byte chunks [verified]

- **Where:** `crates/rouge-syntax/src/incremental.rs:20` (`PARSER_CHUNK_BYTES = 64`), used at line ~296.
- **Change:** hand the parser `&bytes[offset..]`. If the work counter needs a bound, use 16 KiB.
- **Test:** existing incrementality tests still pass. Adjust any work-count assertions that depended on 64-byte granularity, and record why in the PR.

### 0.6 A streamed Markdown list is re-parsed in full on every token [reported]

- **Where:** `crates/rouge-markdown/src/incremental.rs:377`. A trailing list never settles.
- **Change:** settle completed list items, so only the open item stays live. Keep the single parse path.
- **Test:** `crates/rouge-markdown/tests/incremental_test.rs`. Stream a 500-item list token by token and assert parse work grows linearly, using the existing work counters.

### 0.7 Symbols are extracted for every Markdown code block [reported]

- **Where:** `crates/rouge-syntax/src/highlight.rs:39,96` (`analyze()` → `stream.syntax()` runs the tags query eagerly) and `crates/rouge-markdown/src/incremental.rs:85` (`OpenFence::publish`).
- **Change:** compute `SourceSyntax.symbols` lazily on first access. Markdown code blocks then never pay for them.
- Also remove the third copy of each code block's text (`CodeSyntax.stream.source` duplicates `code.text`).

### 0.8 Native host queries the monitor on every redraw [verified]

- **Where:** `crates/rouge/src/platform/native/runner.rs:388` (`current_monitor().refresh_rate_millihertz()`).
- **Change:** cache the refresh rate on `WindowState`, refreshed on `ScaleFactorChanged` and `Moved`.

### 0.9 Only the web host clamps the surface to the maximum texture size [reported]

- **Where:** `crates/rouge/src/driver.rs:148–151` and `platform/web/host.rs:371–374`.
- **Change:** move the clamp into `Driver::resize`, which already rejects zero sizes, and delete the web host's duplicate checks.

### 0.10 Window commands are processed for every window after every event [reported]

- **Where:** `runner.rs:168–185, 260` and `driver.rs:182–199`. Every event flushes every window's `Ui` and allocates a `Vec`.
- **Change:**
  - Process commands only for the window that received the event.
  - Drain into a reusable buffer.
  - Replace the windows `HashMap` with a `Vec<WindowState>` (there are 1–3 windows).

---

## Phase 1: Baseline benchmarks and counters

Phases 3 and 4 need numbers. Extend `crates/rouge/examples/bench.rs` and the `TestUi`-driven benches. Each scenario reports `FrameStats` and `RenderStats` (patches, effects run, memos run, style resolutions, layouts, layer paints, group re-rasters) plus allocation counts from a counting global allocator in the bench binary only.

| Scenario | What it guards |
|---|---|
| One property change in a 5,000-element static UI | work proportional to the change |
| Ten property changes on one element in one batch | 3.1 resolve-once |
| Theme switch across 2,000 buttons | 3.2 |
| Toggling a button's enabled state 100 times | 2.1 (no remount) |
| A focused, idle editor for 10 s | 0.2 (frames rendered ≈ 20, not 1,200) |
| A compositor-only animation for 120 frames | allocations per frame ≈ 0 (Phase 4) |
| A scroll fling over a 10k-row virtual list | regression guard |
| Selection change in a 1,000-row list | 2.4 `selector` |

Add a `style_resolutions` counter to `FrameStats` (needed by 3.1). Record the baseline numbers in the PR description, and re-run them on every Phase 3/4 PR.

---

## Phase 2: User-facing API

### 2.1 Every observable widget property is reactive

- **Problem [verified]:**
  - `button` takes `#[prop(reactive)] label: Prop<String>`, but `disabled: bool` and `selected: bool` are plain bools (`crates/rouge-widgets/src/components.rs:20–31`).
  - So `crates/rouge/examples/shared/settings_view.rs:157–162` wraps a button in `<dynamic>` and rebuilds it to toggle `disabled`, and `crates/rouge-widgets/tests/button_test.rs:30` does the same.
  - `toggle` takes `Signal<bool>`, while `checkbox` takes a `CheckState` closure plus a manual `on_toggle`.
- **Rule:**
  - An observable property accepts a value, a signal or a closure (`IntoProp<T, _>`).
  - An editable value is a `Signal<T>`.
  - `dynamic` is only for structural replacement.
- **Change:**
  - Audit every widget in `crates/rouge-widgets/src`: `components.rs`, `choice.rs`, `slider.rs`, `tabs.rs`, `menu.rs`, `picker.rs`, `input.rs`, `popover.rs`, `progress.rs`, `resizable_panel.rs`. Convert every bool or value prop that can change after mount.
  - `checkbox(checked: Signal<bool>)` for the ordinary case. Tri-state goes through the same widget with a `Signal<CheckState>`; no second widget.
  - Popover owns its trigger toggle (`settings_view.rs:148, 164–165`).
- **Tests:**
  - `crates/rouge-widgets/tests/button_test.rs`: toggle `disabled` through a signal and assert that activation is rejected and then accepted, with `elements_created` unchanged.
  - The same pattern for each converted widget.
- **Done when** no example or test uses `dynamic` to change a widget property.

### 2.2 The builder is the canonical widget API

- **Problem [verified]:** `button` is an 11-parameter positional function with a required test `id` and a matrix of booleans. The pleasant version of the API exists only inside `view!`.
- **Change:**
  - Widgets become constructors that return a concrete stylable type with methods:

    ```rust
    button("Save").disabled(saving).on_click(save)
    ```

  - `#[component]` already turns non-parameter attributes into method calls (`design.md` §6.2), so `view! { <button label="Save" disabled=saving on_click=save/> }` keeps working. The macro maps the required parameter, and the rest become methods.
  - Remove `full_width` (use `.w_full()`).
  - Make `id` optional via `.test_id()`.
  - Fold `keyboard_focusable` into 2.3.
  - Return concrete types (`styled!` forwarding), not `AnyView`, where practical.
  - **Delete** the positional forms. Keeping both APIs violates the "one way" rule.
- **Tests:** rewrite widget tests to the builder form. Add one `view!` test per widget in `crates/rouge-macros/tests/component_test.rs` to show the markup still lowers correctly.
- **Risk:** builders add implementation lines at first. That is only acceptable because the positional, defaulting and conversion paths for widgets go away.

### 2.3 One interaction primitive, keyboard-operable by default

- **Problem:**
  - Activation is implemented three times: `Div::on_press` (`crates/rouge-core/src/view.rs`), `util::pressable` and `util::activates` (`crates/rouge-widgets/src/util.rs:5, 23`) [verified].
  - `button` defaults `keyboard_focusable` to false [verified].
  - State styling is done two ways. `.hover()`/`.active()`/`.focus_visible()` variants exist, but `button` uses `track_interaction` plus a signal, a memo, four effects and five listeners [verified].
- **Change:**
  - A single core primitive, e.g. `Div::control(Control { role, name, focus: FocusPolicy })` plus `.on_activate(f)`. It owns:
    - activation (click, Enter, Space);
    - enabled state (rejects activation and sets the disabled variant);
    - focus policy (`Tab` by default; `Roving` and `None` for toolbars and menus);
    - role and accessible name;
    - pressed, selected and checked states, exposed as variants.
  - Add variant slots for `selected` and `checked`, plus `.when(signal, |s| …)` for arbitrary state.
  - Port `button`, `icon_button`, `checkbox`, `radio`, `toggle`, `tabs`, the menu items and the picker rows.
  - Delete `util::pressable`, `util::activates`, and `track_interaction` uses that only drive styling.
  - Keep `track_interaction` only if a real consumer remains.
- **Tests:** keyboard activation of every control. A disabled control rejects pointer and keyboard activation. A checkbox's label activates the checkbox.
- **Note:** this primarily removes duplicated policy rather than many lines. It is where accessibility attaches later; `design.md:765` lists accessibility as not built.

### 2.4 Small ergonomic additions (each replaces boilerplate in examples)

| Add | Replaces | Evidence |
|---|---|---|
| `Signal<bool>::toggle()` | `x.update(\|v\| *v = !*v)` | 5 sites: `glass_view.rs:59`, `effects_view.rs:128`, `motion_view.rs:81`, `settings_view.rs:62, 148` |
| `ReadSignal<bool>::pick(a, b)` and `.when(sig, \|s\| …)` (2.3) | ~15 `move \|\| if x.get() { A } else { B }` closures | `examples/shared/diff/mod.rs:229–292, 434, 529, 666`; `glass_view.rs:56`; `effects_view.rs:124` |
| `IntoProp` in the prelude | `use rouge::core::IntoProp` in user helpers | `diff/mod.rs:10, 716–726` |
| Window clear colour derived from the root background or the theme's `canvas` | stating the background twice | `demo.rs:49, 56`; `glass_view.rs:30` |
| `selector(sig) -> Selector<K>` with `.is(key) -> ReadSignal<bool>` (O(1) per change) | per-row `memo(selected == Some(x))` (O(rows)) | `diff/mod.rs:276, 543, 612` [verified at 612] |
| `action(async …)` with `.dispatch()`, `.pending()`, `.value()` | `resource` keyed on a bool plus `flatten` | `settings_view.rs:129–142` |

- `WindowOptions::background` is deleted once the clear colour is derived.
- `selector` lives in `rouge-reactive`, keeps a key-to-signal map, and notifies only the old and new keys.

### 2.5 Simplify the Markdown widget

- **Problem [verified]:** `markdown(rows: ReadSignal<MarkdownRows>, paint: MarkdownPaint)`, where `MarkdownPaint` needs palette and syntax memos plus an action callback, and `MarkdownRows` exposes manual sync against `MarkdownStream` (`crates/rouge-widgets/src/markdown.rs:27`).
- **Change:**
  - Default path: `markdown(document)` / `markdown(stream)` with `.on_link(f)` and `.on_action(f)`. The widget owns row projection, syncing and the theme.
  - Row-level APIs stay available for annotated and virtualized use and share the same implementation.
  - Remove `rows.entries.clone()` on every sync (line ~63) and the boxed per-row `background()` closure (line ~230); rows hold `Arc` slices of the document (5.2).
  - Move code-review-specific policy out of `rouge-markdown/src/document.rs`: comment targets, per-code-line `display_label` strings, and the duplicate `outline`. Expose them as optional projections computed on demand.
- **Check first:** the sibling terminal frontend depends on `rouge-markdown`. Confirm which of these fields it reads before moving them.

---

## Phase 3: Core internals

Phase 1 numbers are required before and after every item here.

### 3.1 One write path, resolved once per element

- **Problem [verified]:**
  - `Div::bind_prop` (`crates/rouge-core/src/view.rs`) carries two paths per property: an `init` closure that writes `ElementSpec`, and a `patch` closure that builds a `Patch`.
  - Gluing them costs per dynamic prop: an `Rc<Cell<Option<ElementId>>>`, an `Rc<Cell>` for the initial value, a boxed `Binding` and a cloned `Queue`.
  - Each `Patch::Style` then clones the element's whole effective `Style` (including `Vec<BoxShadow>` and `Cow` font), plus the taffy `Style` for layout properties, before resolving (`crates/rouge-core/src/tree/cascade.rs:14–30`).
  - Ten changes to one element mean ten full resolutions.
- **Change:**
  1. **Build:** create the element first, then run each binding's first evaluation and apply it through the same apply path, inline, with no queueing. This keeps the design rule "mounting costs one write, no follow-up patch". Delete every `init` closure, `ElementSpec`'s duplicate fields where `Element` already holds them, and the `initial` and `element` cells.
  2. **Update:** `Patch::Style`/`Variant`/`Text` write the declaration into element state and push the id onto a deduplicated dirty-style set (a flag on the element plus a `Vec<ElementId>`).
  3. **Resolve:** once per flush, resolve each dirty element's effective style, layout style and inherited text, in tree order so text inheritance propagates once.
  4. **Order is preserved:** declarations are applied in the order they were written, so `.p()` then `.px()` still works. Structural patches (`Build`), focus, edit and paste stay strictly FIFO and resolve pending styles before they run.
  - Make `Patch::Text` a plain-data enum instead of `Box<dyn FnOnce(&mut TextData)>`.
- **Tests:**
  - `crates/rouge-core/tests/style_test.rs`: ten bound props changed in one batch → `style_resolutions == 1`.
  - `.p()` then `.px()` ordering.
  - An inherited text-size change resolves the subtree once.
  - Every existing style, variant and cascade test still passes.
- **Expected:** fewer lines in `view.rs` and `tree/cascade.rs`, and far fewer allocations per mount.

### 3.2 Theme roles resolved at paint time

- **Problem [verified]:**
  - Every `theme.accent()` call creates a new memo node (`crates/rouge-core/src/theme.rs:16`), and each `.bg(theme.x())` adds a binding effect on top.
  - Every text leaf creates its own effect on the whole palette to push `Patch::RefreshText` (`Div::mount` in `view.rs`).
  - The role list is duplicated between `rouge-theme` and `core/theme.rs:59`.
- **Step A (safe interim):** cache one memo per role per `Theme` (created lazily, stored with the theme's signal), so repeated calls return the same node. Measure with the theme-switch benchmark.
- **Step B (target):**
  - Add `enum Fill { Color(Color), Role(Role) }`, with `Role` generated from a single role table exported by `rouge-theme`.
  - `.bg`, `.text_color` and `.border_color` accept `Fill` via `PropFrom`; `theme.accent()` returns `Fill::Role(Role::Accent)`, a plain value.
  - Paint resolves roles against the current palette.
  - A theme change sets `repaint_all` and clears text-colour caches. No effects run.
  - Delete: the role memos, the per-text-leaf theme effect, `Patch::RefreshText`, the duplicated role list, and the `UiPalette<T>` generic (store `Rgba`; convert at paint).
  - `control_style` / `selection_style` / `notice_style` return `Fill`s.
- **Risk:** reactive code that reads a role's colour value (not just styles with it) needs `theme.color(Role)` → `Memo<Color>`. Keep that as the single explicit escape hatch.
- **Tests:**
  - `crates/rouge-core/tests/theme_test.rs`: a theme switch across 2,000 buttons runs zero effects and repaints once.
  - A role-styled hover variant updates on a theme switch.
  - Text colour inheritance still resolves the role.

### 3.3 An explicit frame contract

- **Problem [verified]:**
  - `Ui::step` (`crates/rouge-core/src/ui.rs:217`) runs flush ×3, publish ×3 and layout ×2, because geometry observers (`track_size`, `track_bounds`, `track_paint_region`, `scroll_state`, editor selections) write back into signals, which create patches, which force a relayout.
  - The contract is encoded only as a call sequence.
- **Change:**
  - Make the phases named and explicit:

    ```text
    input/tasks → mutations (flush) → animate → layout → bounded measurement feedback → paint → place surfaces → compose
    ```

  - Split observers into two kinds:
    - **Measurement feedback** must settle before paint (virtual-list range, measured rows). It runs inside layout, bounded to one re-layout, as now. Keep `FrameStats::relayouts`.
    - **Notifications** (`track_size`, `track_bounds`, `track_paint_region`, scroll state publishing) publish after paint. Their patches apply next frame.
  - Prefer on-demand queries where they work, as `VirtualListHandle` already does. Audit each observer's consumers and convert the ones that only read on events.
  - `Wake` from 0.2 is the output of this contract.
  - Never add an unbounded "loop until quiet".
- **Tests:**
  - `crates/rouge-core/tests/ui_test.rs`: a frame with no observers runs exactly one layout.
  - A virtual list whose range changes settles in one frame.
  - A `track_size` consumer sees the new size by the next frame.

### 3.4 Remove mirror structures (after measurement)

- **Five parallel structures today:**
  - the reactive scope tree;
  - the element `SlotMap`;
  - the fragment `SlotMap`;
  - taffy's `TaffyTree`;
  - the compositor's `HashMap<LayerId, Layer>`.
- **Changes:**
  - **Taffy:** implement taffy's low-level traits (`TraversePartialTree`, `LayoutPartialTree`, `CacheTree`, `LayoutFlexboxContainer`, `LayoutGridContainer`, `LayoutBlockContainer`) on `Tree`. Delete `TaffyTree<ElementId>`, `Element::taffy`, and the taffy style clone in `cascade.rs`. Store the layout style and taffy's cache on `Element`.
  - **Compositor layers:** replace the std SipHash `HashMap<LayerId(u64), Layer>` (`compositor.rs:431`) and `Motions` with `SlotMap`s, and make `LayerId` a slotmap key. Drop `Tree::next_layer`. Its `sources: HashMap<u64, LayerId>` becomes keyed by signal id.
  - **Scroll identity [verified]:** `scroll_key` hashes a `Signal` through `DefaultHasher` into a `u64` (`crates/rouge-core/src/scroll.rs:31`), which can collide. Use the signal's id directly. This needs a `pub(crate)`-visible id accessor in `rouge-reactive` or a `Hash`-free key type.
  - **Other `HashMap`s:** switch the remaining std ones in `rouge-core` (`control.rs`, `scroll.rs`, `tree.rs` handles and motions, `tree/paint.rs`, `scroll/geometry.rs`, `scroll/heights.rs`) to FxHash or slotmap keys. That makes `design.md` §9.2 true.
- **Tests:** existing layout, scroll and compositor tests. Benchmark layout of a 5,000-element tree before and after.

### 3.5 Reactive runtime: per-read cost (profile first)

- The runtime is under 2k lines; it is not the size problem.
- **Per-read cost [verified]:** a `get()` today:
  - clones an `Rc<Graph>` out of a thread-local (`with_rt`, `crates/rouge-reactive/src/runtime.rs`);
  - takes several `RefCell` borrows;
  - clones an `Rc<RefCell<dyn Any>>`;
  - downcasts to `Option<T>`.
- **Per-node cost:** two `IndexSet`s and a `HashMap`.
- **Other costs:**
  - `track` does a linear `contains`;
  - `update_if_necessary` clones the `sources` Vec;
  - `mark_subscribers` allocates a stack on every write;
  - `Stored`, `Callback` and `Emitter` are full graph nodes with subscriber sets.
- **Candidates, each landed only with a benchmark win:**
  - Store values inline in the node slot (`Box<dyn Any>`), not as an `Rc<RefCell>`.
  - Borrow the graph from the thread-local without cloning the `Rc`. Re-entrancy must be preserved; computations re-enter the graph.
  - Use `SmallVec<[NodeId; 2]>` for subscribers and sources.
  - Use a reusable scratch stack in `mark_subscribers`.
  - Give `Stored` and `Callback` a separate untracked arena (no subscribers or children), still scope-owned and disposed with the scope.
- **Tests:** the existing `crates/rouge-reactive/tests` suite unchanged, plus a micro-bench of 1M `get()` calls and a 10k-node fan-out write.

---

## Phase 4: Frame representation (compositor → renderer)

Prototype on a branch and merge only if the layers, glass and scroll benchmarks are equal or better.

- **Problem:**
  - `Compositor::scene` builds a fresh `Vec<Item>` each frame, with a nested `Vec` per group (`compositor.rs:877, 931`; `renderer.rs:82–90`).
  - The renderer then builds a second representation (its `Plan`, ~line 650) and walks the tree an extra time just to fold surface frame ids into group signatures (`hash_frames`, ~1891–1910, with cursor juggling at ~1242–1265) [reported].
- **Change:**
  - Make one reusable, flat command list owned by the compositor (`DrawList(ref) | BeginGroup(..) | EndGroup | Surface(..)`), cleared and refilled each frame without reallocating.
  - Or have the compositor traversal call the renderer's planning interface directly, if that keeps `rouge-render` free of `rouge-core` types.
  - Fold surface frame ids into `summarize` (`compositor.rs:1086`) and delete `hash_frames`.
  - This replaces both the nested scene and the plan's intermediate types (`Sampled`, `GroupContent`, `Placed` → `Content`, item 5.1). It must not become a third representation.
- **Must keep:**
  - group opacity semantics;
  - backdrop ordering and pass-break merging;
  - scissor clipping;
  - surface lifetimes and latching;
  - group cache invalidation (after 0.1 and 0.4).
- **Done when:** zero allocations per frame during a compositor-only animation (the Phase 1 scenario).

---

## Phase 5: Per-crate cuts

Land these alongside nearby work. Estimates come from the reviews.

### 5.1 `rouge-render` (~450–550 LOC)

- [reported] **Surface buffer pool in the producer.**
  - `SurfaceProducer::configure(desc)` plus `acquire() -> Option<(BufferRef, &Texture)>` and `submit(buffer)`, with `Released` handled internally.
  - `BufferSetId`, `register` and `unregister` become internal.
  - Delete the duplicated chains in `src/testing.rs:117–305` and `crates/rouge/examples/surface.rs:95–160`.
  - Fix the example's hardcoded `Bgra8Unorm` (line ~171).
- [reported] **Defer fences, timed queues (`present_at`) and the `Presented`/`Discarded` events** until a real producer needs them; only `rouge-core/tests/surface_test.rs` uses them.
  - **This is a product decision:** `design.md` §12.3–12.6 specifies them. If they stay, collapse `Acquire` and `Pending.ready` into `fence: Option<Fence>`.
- [reported] **Fold types together:**
  - `Placement` into `SurfaceConfig`.
  - The `binds` `HashMap` → a counter on `Entry`.
  - `InteropError` into `SurfaceError`.
  - `NativeBuffer` (a single cfg'd variant) → a cfg'd field.
  - Delete the manual `Debug` for `SurfaceEvent`.
  - Switch std `HashMap` to FxHash.
- [reported] **Plan stage:**
  - Collapse `Sampled`/`GroupContent`/`Placed` (~696–718) into direct `Content`.
  - Put source and destination on `BlurPass` and remove the `Snapshot → snapshots → blur_passes` indirection (~599–670).
  - `Kernel` uses `levels == 0` instead of `Option<Kernel>`.
  - Reduce `Window`/`Area`/`Mapping` (~236–392) to functions over `Rect`.
- [reported] **One `ensure_target(slot, dims) -> bool` helper** for the six "make sure a texture of this size exists" sites (~1152, 1183, 1393, 1458, 1575, 1831).
- [reported] **Per-frame waste:**
  - Run `images.evict`, `instances.maintain` and `surface_binds.retain` every N frames, not every frame.
  - `DrawList::replace` moves Vecs instead of copying.
  - `Surfaces::maintain` keeps `retired`'s capacity.
- [reported] **Small fixes:**
  - `Glass::frosted` uses `..Glass::new()`.
  - Drop `From<&T> for EffectInstance`.
  - Remove the second `Device` in `Textures`.
  - The test-pattern pipeline uses the auto layout and stops allocating per present.
  - Fold `tests/common::Canvas` into `Headless`.

### 5.2 `rouge-markdown` (~450 LOC)

- [reported] **`MarkdownText` (text plus mark and link ranges) is the only inline representation.**
  - Build it straight from parser events with a mark-bitset stack, and run linkify once per non-link run.
  - Delete `MarkdownInline`, `inline_children`/`inline_node`/`autolink`/`split_links`, the recursion in `text.rs:93–131`, and the widget's `flatten`.
  - Blocks hold `Arc<MarkdownText>`.
- [reported] **Remove the `Node` event tree** (`document.rs:433–493`): no `into_static()`, and a single-pass stack builder over the borrowed iterator.
- [reported] **Slim the model:**
  - Labels become methods computed on demand.
  - Drop `MarkdownCodeLine.source_line` and `.index` (they are derivable).
  - Drop `MarkdownCodeBlock.language` (derive it from `info`).
  - The outline becomes a filter over targets.
- [reported] **Drop `source_styles`** (`document.rs:322–337, 495–519`). Source view highlights through rouge-syntax's Markdown grammar.
- [reported] **Resolve settled reference definitions in the existing broken-link callback.** Delete the prefix-string concatenation and range shifting (`incremental.rs:27–38, 276–286, 335–349, 519–560`).
- [reported] **Merge `MarkdownStream` into `IncrementalDocument`.** `stream.rs` is mostly forwarding.
- **Check first:** confirm the terminal frontend's usage before removing public items.

### 5.3 `rouge-syntax` (~200 LOC)

- [reported] **One language table:** one macro row per language (name, extensions, grammar) resolving to a `u8` `LanguageId`.
  - It replaces `canonical_id`, `extension_id` and `fallback_id` (`language.rs:62–177`) plus the `grammars!` list (`incremental.rs:584`).
  - `InjectedDocument.language` and `Injection.language` become ids.
  - `LanguageHint` becomes `&str`.
- [reported] **`DocumentHighlights`:** one `Vec<(u32, u32, HighlightId)>` instead of three parallel arrays. Delete the `SpanParts`/`HighlightSpans` aliases and both `IntoIterator` impls (`highlight.rs:384–446`).
- [reported] **Drop the dead `Span.highlight: Option`.** First check whether `None` was meant to mask a range; if so, fix the latent bug instead.
- [reported] **`flatten`/`normalize`** reuse scratch buffers on the stream.
- [reported] **Delete the `THREAD_WORK` thread-local** (`highlight.rs:21`) in favour of `stream.work_stats()`.
- [reported] **Drop `SyntaxTheme.captures`.**
- [reported] **Fix the fragile `query_source.contains(kind)` substring check** in the error-prefix comparison.

### 5.4 `rouge-theme` (~100 LOC)

- [reported] **Move `PickerState` and `FuzzyFilter` to `rouge-widgets`,** so nucleo stops leaking into syntax and markdown through the `Rgba` dependency. Delete the fake `Clone`/`PartialEq`/`Debug` impls (`fuzzy.rs:44–62`).
  - **Check first:** confirm the terminal frontend doesn't use the picker; if it does, give it a small crate of its own.
- [verified] **`UiPalette::default()` is a third palette** (dark green; `palette.rs:249`). Make `Default` equal `light()` and drop `SCRIM_ALPHA` if it becomes unused.
- **Drop the `<T>` generic** once 3.2 lands.
- [reported] **Delete the unused `MarkdownPalette::colors()`.**
- **Move widget-only vocabulary** (`ControlSize`, `CheckState`, `ModalSize`) to `rouge-widgets`, after checking the terminal frontend.

### 5.5 `rouge` facade and hosts (~250 LOC)

- [reported] **`Driver::open(...)`** performs the shared setup sequence. Today it is repeated at `runner.rs:137–156` and `host.rs:212–224`.
- [reported] **Decide the default Edit-menu policy once.** Native always adds it (`runner.rs:43–46`); web adds it only when the app supplied menus (`host.rs:61–63`).
- [reported] **Fold `BrowserRuntime` (`platform/web/runtime.rs`) into `Host`:** a `Cell<bool>` wake flag, and delete the second registry and the `slotmap` dependency.
- [reported] **Web input:** one listener per target with a single `match` on the event type, mirroring native (`host.rs:500–659`).
- [reported] **`Drawn` → `bool`** (`driver.rs:26–31`).
- [reported] **Profiling self-contained:**
  - `Profile::after_frame` logs its own summary.
  - `ROUGE_PRESENT_MODE` and `ROUGE_EXIT_AFTER_FRAMES` move into `ProfileSettings::read`.
  - Don't format `environment` when profiling is off (`driver.rs:113`).
- [reported] **One path to each item:**
  - Delete `App::appearance()` and `App::quit()` (`app.rs:118–124`).
  - Delete the root re-exports that duplicate the prelude (`lib.rs:10–13`).
  - `#[doc(hidden)]` the test-only `platform::{Wheel, logical_point, named_key, …}` and `web::{dom_key, BrowserRuntime}`.
- [reported] **`persist.rs`** gets its own feature, or moves into the app that uses it (no in-repo consumer).
- **Optional:** `WindowOptions` as a plain struct with public fields (~40 LOC of setters).

### 5.6 `rouge-core` and `rouge-widgets` (beyond Phases 2–3)

- **Move test queries off `Ui` and onto `TestUi`:** `bounds_of`, `bounds_of_all`, `text_of`, `text_bounds`, `text_index_at`, `bounds_at` (`ui.rs:405–435`).
- **`Declaration::Shadows(Vec<BoxShadow>)`:** use a small fixed array, or `Rc<[BoxShadow]>`, to avoid a clone on every resolution.
- **Reactive extras:** `Emitter` = `Stored<Vec<Callback>>`, cloned on every `emit`. Rebuild it on the 3.5 untracked arena, or delete it if 2.3 removes its consumers.

---

## Phase 6: Docs and examples

- **Update `design.md` in the same PR as each change**, especially:
  - §4.2 (frame steps, `Wake`);
  - §5 (`selector`, `action`, `toggle`);
  - §6 (builder-first widgets, `Fill`, `.when`);
  - §9.2 (hash maps);
  - §12 (producer pool, deferred fences);
  - §15 (widget API);
  - §16 (Markdown widget).
- **Examples teach one way:**
  - `motion_view.rs:79–86` uses `.transition()`, not a manual `motion` plus `animate_to`.
  - The settings example has no `dynamic` around widgets.
  - The diff example uses `selector`, `pick` and `.when`.
  - The surface example uses `acquire`/`submit`.
- **Bad-shape test:** add an example-level smoke test (headless `TestUi`) for each example so API changes break loudly.

## Out of scope (separate decisions)

- Dropping features to reach a much smaller codebase (backdrop effects, surfaces, the web host, rich text editing, incremental documents).
- New features: Servo/web view, a render thread, more compositor effects. Hold these until Phase 2 is done.
- Replacing fine-grained signals or the retained compositor.

## Tracking

| Item | Owner | PR | Bench before | Bench after | Status |
|---|---|---|---|---|---|
| 0.1 | | | | | |
| 0.2 | | | | | |
| … | | | | | |

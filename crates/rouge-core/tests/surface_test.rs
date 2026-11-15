mod common;

use common::{Stage, StageBuilder};
use rouge_core::testing::{
    BLUE, GREEN, LINEAR, RED, TestResult, TestUi, TestUiBuilder, WHITE, assert_near,
};
use rouge_core::{
    Color, ColorMatrix, EffectSlot, Glass, LayerProp, Nested, Pose, Rect, ScrollState, Scrollable,
    Styled, SurfaceError, SurfaceSource, SurfaceStatus, View, div, scroll, show, surface, text,
};
use rouge_reactive::{Runtime, signal};
#[cfg(target_os = "macos")]
use rouge_render::Sharing;
use rouge_render::testing::{Pattern, TestSurface, TestSurfaceBuilder, TestSurfaceError};
use rouge_render::{
    AlphaMode, BufferDesc, DrawList, Frame, GpuError, Origin, Point, SurfaceConfig, SurfaceEvent,
    SurfaceId, surface_channel,
};
use std::cell::Cell;
use std::error::Error;
use std::time::{Duration, Instant};
use wgpu::{TexelCopyBufferLayout, TextureFormat};

const QUADRANTS: Pattern = Pattern::quadrants([RED, GREEN, BLUE, WHITE]);

#[test]
fn a_surface_is_laid_out_like_a_leaf_and_shows_its_producers_frames() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut producer, source) = TestSurface::new();
    let mut ui = TestUiBuilder::default()
        .size(100.0, 100.0)
        .scale(2.0)
        .build(|| div().p(10.0).child(surface(source).w(80.0).h(40.0)));
    ui.draw()?;
    producer.pump()?;
    assert_eq!(config(&producer)?, (160, 80, 2.0, true));
    producer.present(Pattern::quadrants([RED, GREEN, BLUE, WHITE]))?;
    let image = ui.render()?;
    assert_near(image.pixel(40, 40), RED, "top left");
    assert_near(image.pixel(160, 90), WHITE, "bottom right");
    assert_near(image.pixel(10, 10), [0, 0, 0, 0], "padding");
    assert_eq!(ui.bounds(50.0, 30.0)?, Rect::new(10.0, 10.0, 80.0, 40.0));
    Ok(())
}

#[test]
fn a_new_frame_costs_no_reactive_layout_or_paint_work() -> TestResult {
    let _rt = Runtime::new().enter();
    let (mut producer, mut ui) = filled(|source| surface(source).size(100.0))?;
    producer.present(Pattern::Solid(RED))?;
    ui.render()?;
    let before = ui.stats();
    producer.present(Pattern::Solid(BLUE))?;
    ui.advance(0.016);
    assert_near(ui.pixel(50, 50)?, BLUE, "the new frame");
    let after = ui.stats();
    assert_eq!(
        (after.layouts, after.paints, after.reactive.effects_run),
        (before.layouts, before.paints, before.reactive.effects_run)
    );
    Ok(())
}

#[test]
fn resizing_reconfigures_the_producer_and_stretches_the_old_frame() -> TestResult {
    let _rt = Runtime::new().enter();
    let width = signal(50.0);
    let (mut producer, mut ui) =
        filled(move |source| surface(source).w(move || width.get()).h(100.0))?;
    producer.present(Pattern::quadrants([RED, GREEN, BLUE, WHITE]))?;
    ui.render()?;
    width.set(100.0);
    ui.advance(0.0);
    let image = ui.render()?;
    assert_near(image.pixel(75, 25), GREEN, "the old frame stretches");
    producer.pump()?;
    assert_eq!(config(&producer)?, (100, 100, 1.0, true));
    Ok(())
}

#[test]
fn hidden_clipped_and_occluded_surfaces_are_not_visible() -> TestResult {
    let _rt = Runtime::new().enter();
    let state = signal(ScrollState::default());
    let (mut producer, mut ui) = filled(move |source| {
        scroll()
            .size(100.0)
            .scroll_state(state)
            .child(div().h(100.0))
            .child(surface(source).size(100.0))
    })?;
    assert!(!config(&producer)?.3, "scrolled out of view");
    state.update(|s| s.scroll_to(Point::new(0.0, 100.0)));
    ui.advance(0.0);
    ui.draw()?;
    producer.pump()?;
    assert!(config(&producer)?.3, "scrolled into view");
    ui.set_visible(false);
    ui.draw()?;
    producer.pump()?;
    assert!(!config(&producer)?.3, "the window is occluded");
    let (invisible, _ui) = filled(|source| surface(source).size(100.0).invisible())?;
    assert!(!config(&invisible)?.3, "the element is invisible");
    Ok(())
}

#[test]
fn a_failed_surface_shows_its_fallback_and_reports_its_status() -> TestResult {
    let _rt = Runtime::new().enter();
    let status = signal(SurfaceStatus::Waiting);
    let (mut producer, mut ui) = filled(move |source| {
        surface(source)
            .size(100.0)
            .status(status)
            .fallback(|error| div().test_id("fallback").child(text(error.to_string())))
    })?;
    producer.present(Pattern::Solid(RED))?;
    ui.draw()?;
    ui.advance(0.0);
    assert_eq!(status.get_untracked(), SurfaceStatus::Live);
    assert!(ui.rect_of("fallback").is_err());
    producer.fail(SurfaceError::Producer("no GPU".into()));
    ui.draw()?;
    ui.advance(0.0);
    assert_eq!(
        status.get_untracked(),
        SurfaceStatus::Failed(SurfaceError::Producer("no GPU".into()))
    );
    assert!(ui.rect_of("fallback").is_ok());
    Ok(())
}

#[test]
fn removing_a_surface_detaches_the_producer_after_its_exit() -> TestResult {
    let _rt = Runtime::new().enter();
    let open = signal(true);
    let (mut producer, mut ui) = filled(move |source| {
        let source = Cell::new(Some(source));
        show(
            move || open.get(),
            move || source.take().map(|source| surface(source).size(100.0)),
        )
        .exit(Pose::new().opacity(0.0), LINEAR)
    })?;
    producer.present(Pattern::Solid(RED))?;
    ui.render()?;
    open.set(false);
    ui.advance(0.0);
    ui.draw()?;
    producer.pump()?;
    assert!(producer.is_detached());
    ui.advance(0.5);
    assert!(ui.pixel(50, 50)?[0] > 0, "the last frame fades out");
    assert_eq!(ui.surfaces()?.stats().surfaces, 1);
    ui.run(1.0);
    ui.draw()?;
    assert_eq!(ui.surfaces()?.stats().surfaces, 0);
    Ok(())
}

#[test]
fn a_presented_frame_is_sampled_in_place_and_costs_no_upload() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present(QUADRANTS)?;
    let first = stage.draw();
    let image = stage.image()?;
    assert_near(image.pixel(25, 25), RED, "top left");
    assert_near(image.pixel(75, 25), GREEN, "top right");
    assert_near(image.pixel(25, 75), BLUE, "bottom left");
    assert_near(image.pixel(75, 75), WHITE, "bottom right");
    assert_eq!(first.surfaces, 1);
    assert_eq!(stage.status(), SurfaceStatus::Live);

    surface.present(Pattern::Solid(BLUE))?;
    let second = stage.draw();
    assert_near(stage.image()?.pixel(25, 25), BLUE, "the next frame");
    assert_eq!(
        (second.instance_bytes_uploaded, second.image_bytes_uploaded),
        (0, 0),
        "a new surface frame uploads nothing"
    );
    Ok(())
}

#[test]
fn bottom_left_frames_are_flipped_while_sampling() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present_flipped(QUADRANTS, |frame| frame.origin(Origin::BottomLeft))?;
    stage.draw();
    let image = stage.image()?;
    assert_near(image.pixel(25, 25), RED, "top left");
    assert_near(image.pixel(25, 75), BLUE, "bottom left");
    Ok(())
}

#[test]
fn alpha_modes_blend_premultiplied_pixels_or_ignore_alpha() -> TestResult {
    let half = [0, 0, 128, 128];
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present(Pattern::Solid(half))?;
    stage.draw();
    assert_near(
        stage.image()?.pixel(50, 50),
        [127, 127, 255, 255],
        "over white",
    );

    surface.present_frame(Pattern::Solid(half), |frame| frame.alpha(AlphaMode::Opaque))?;
    stage.draw();
    assert_near(stage.image()?.pixel(50, 50), [0, 0, 128, 255], "opaque");
    Ok(())
}

#[test]
fn the_region_selects_part_of_the_buffer_and_stretches_it() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present_frame(QUADRANTS, |frame| {
        frame.region(Rect::new(50.0, 0.0, 50.0, 50.0))
    })?;
    stage.draw();
    let image = stage.image()?;
    assert_near(image.pixel(5, 5), GREEN, "top left of the region");
    assert_near(image.pixel(95, 95), GREEN, "the region fills the layer");
    Ok(())
}

#[test]
fn a_surface_layer_is_clipped_transformed_and_faded_like_any_layer() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    for prop in LayerProp::RADII {
        stage.set(stage.child, prop, 30.0);
    }
    stage.set(stage.child, LayerProp::Scale, 0.5);
    stage.set(stage.child, LayerProp::Opacity, 0.5);
    surface.present(Pattern::Solid(RED))?;
    stage.draw();
    let image = stage.image()?;
    assert_near(image.pixel(10, 10), WHITE, "outside the scaled layer");
    assert_near(image.pixel(27, 27), WHITE, "the rounded corner is clipped");
    assert_near(image.pixel(50, 50), [255, 128, 128, 255], "half opacity");
    Ok(())
}

#[test]
fn a_content_effect_over_a_surface_refreshes_on_every_frame() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    stage.effect(
        stage.child,
        EffectSlot::Content(0),
        &ColorMatrix::IDENTITY,
        false,
    );
    surface.present(Pattern::Solid(RED))?;
    stage.draw();
    assert_near(stage.image()?.pixel(50, 50), RED, "first frame");
    surface.present(Pattern::Solid(BLUE))?;
    let stats = stage.draw();
    assert_near(
        stage.image()?.pixel(50, 50),
        BLUE,
        "the cached group re-rendered",
    );
    assert_eq!(stats.group_passes, 1);
    Ok(())
}

#[test]
fn glass_over_a_surface_does_not_reuse_a_stale_backdrop() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    let glass = stage.compositor.create_layer();
    stage
        .compositor
        .set_frame(glass, Rect::new(20.0, 20.0, 60.0, 60.0));
    stage.effect(glass, EffectSlot::Backdrop, &Glass::frosted(8.0), false);
    stage.content(glass, vec![DrawList::new()], vec![]);
    let nested = [stage.child, glass].map(|layer| Nested { layer, clip: None });
    stage.content(
        stage.root,
        (0..3).map(|_| DrawList::new()).collect(),
        nested.to_vec(),
    );
    surface.present(Pattern::Solid(RED))?;
    stage.draw();
    let before = stage.image()?.pixel(50, 50);
    stage.draw();
    surface.present(Pattern::Solid(BLUE))?;
    let stats = stage.draw();
    let after = stage.image()?.pixel(50, 50);
    assert!(
        before[0] > after[0] && after[2] > before[2],
        "{before:?} -> {after:?}"
    );
    assert_eq!(stats.backdrops_reused, 0);
    Ok(())
}

#[test]
fn the_mailbox_keeps_the_newest_frame_and_returns_the_rest() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present(Pattern::Solid(RED))?;
    surface.present(Pattern::Solid(BLUE))?;
    stage.draw();
    assert_near(stage.image()?.pixel(50, 50), BLUE, "the newest frame");
    surface.pump()?;
    assert_eq!(
        surface.free_buffers(),
        2,
        "the discarded buffer is free again"
    );
    assert_eq!(stage.surfaces().stats().frames_discarded, 1);
    Ok(())
}

#[test]
fn a_replaced_buffer_is_released_after_the_gpu_finishes_with_it() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100).buffers(2))?;
    surface.present(Pattern::Solid(RED))?;
    stage.draw();
    surface.present(Pattern::Solid(BLUE))?;
    assert!(matches!(
        surface.present(Pattern::Solid(RED)),
        Err(TestSurfaceError::NoFreeBuffer)
    ));
    stage.draw();
    stage.headless.wait()?;
    stage.surfaces().maintain();
    surface.pump()?;
    assert_eq!(surface.free_buffers(), 1);
    surface.present(Pattern::Solid(RED))?;
    Ok(())
}

#[test]
fn frame_requests_are_answered_after_presenting_only_while_visible() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    let now = Instant::now();
    let interval = Duration::from_millis(8);
    surface.request_frame();
    stage.surfaces().latch();
    assert!(stage.surfaces().needs_frame());
    stage.surfaces().presented(now, interval);
    assert!(!stage.surfaces().needs_frame());
    let events = surface.pump()?.to_vec();
    assert!(events.iter().any(
        |event| matches!(event, SurfaceEvent::FrameRequest { target } if *target == now + interval)
    ));

    place(&mut stage, surface.id(), false);
    surface.request_frame();
    stage.surfaces().latch();
    assert!(
        !stage.surfaces().needs_frame(),
        "hidden surfaces are not paced"
    );
    Ok(())
}

#[test]
fn configuration_changes_are_sent_once() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder())?;
    assert!(surface.config().is_some());
    place(&mut stage, surface.id(), true);
    stage.surfaces().latch();
    assert!(
        surface.pump()?.is_empty(),
        "an unchanged placement is not resent"
    );
    place(&mut stage, surface.id(), false);
    stage.surfaces().latch();
    let events = surface.pump()?.to_vec();
    assert_eq!(events.len(), 1);
    let config = surface.config().ok_or("no config")?;
    assert!(!config.visible);
    assert_eq!((config.width, config.height), (100, 100));
    Ok(())
}

#[test]
fn a_detached_surface_keeps_its_last_frame_until_its_layer_is_gone() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present(Pattern::Solid(RED))?;
    stage.draw();
    stage.surfaces().detach(surface.id());
    stage.draw();
    surface.pump()?;
    assert!(surface.is_detached());
    assert_near(stage.image()?.pixel(50, 50), RED, "the last frame stays");
    assert_eq!(stage.surfaces().stats().surfaces, 1);

    stage.compositor.remove_layer(stage.child);
    stage.draw();
    stage.headless.wait()?;
    stage.surfaces().maintain();
    assert_eq!(stage.surfaces().stats().surfaces, 0);
    surface.pump()?;
    assert_eq!(surface.free_buffers(), 3, "the last buffer is released");
    Ok(())
}

#[test]
fn a_failing_producer_reports_its_error() -> TestResult {
    let (surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.fail(SurfaceError::Producer("the page crashed".into()));
    stage.draw();
    assert_eq!(
        stage.status(),
        SurfaceStatus::Failed(SurfaceError::Producer("the page crashed".into()))
    );
    Ok(())
}

#[test]
fn srgb_buffers_are_rejected() -> TestResult {
    let builder = TestSurface::builder()
        .size(100, 100)
        .format(TextureFormat::Bgra8UnormSrgb);
    let (mut surface, source) = builder.build();
    let mut stage = StageBuilder::default()
        .sized(100.0, 100.0)
        .surface(source)
        .build()?;
    stage.draw();
    assert!(matches!(
        surface.pump(),
        Err(TestSurfaceError::Surface(SurfaceError::Format(
            TextureFormat::Bgra8UnormSrgb
        )))
    ));
    Ok(())
}

#[test]
fn a_producer_can_drive_the_protocol_directly() -> TestResult {
    let (mut producer, source) = surface_channel();
    let mut stage = staged(source)?;
    stage.draw();
    assert_eq!(producer.try_recv(), Some(SurfaceEvent::Attached));
    assert!(matches!(
        producer.try_recv(),
        Some(SurfaceEvent::Configure(_))
    ));
    producer.configure(BufferDesc::new(4, 4).count(1))?;
    let queue = producer
        .cx()
        .ok_or("the producer was not attached")?
        .queue()
        .clone();
    let (buffer, shared) = producer.acquire().ok_or("no free buffer")?;
    let pixels: Vec<u8> = [0u8, 0, 255, 255].repeat(16);
    queue.write_texture(
        shared.texture().as_image_copy(),
        &pixels,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: None,
        },
        shared.texture().size(),
    );
    assert!(producer.acquire().is_none(), "the only buffer is in use");
    producer.submit(Frame::new(buffer));
    stage.draw();
    assert_near(
        stage.image()?.pixel(50, 50),
        RED,
        "BGRA bytes from the producer",
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn io_surface_buffers_are_rendered_into_and_sampled_without_copies() -> TestResult {
    let builder = TestSurface::builder()
        .size(100, 100)
        .sharing(Sharing::Native);
    let (mut surface, mut stage) = scene(builder)?;
    assert!(surface.buffers().iter().all(|b| b.io_surface().is_some()));
    surface.present(QUADRANTS)?;
    stage.draw();
    let image = stage.image()?;
    assert_near(image.pixel(25, 25), RED, "top left");
    assert_near(image.pixel(75, 75), WHITE, "bottom right");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn an_io_surface_written_by_the_producer_is_shown_as_is() -> TestResult {
    let (mut producer, source) = surface_channel();
    let mut stage = staged(source)?;
    stage.draw();
    assert_eq!(producer.try_recv(), Some(SurfaceEvent::Attached));
    producer.configure(BufferDesc::new(8, 8).count(1).sharing(Sharing::Native))?;
    let (buffer, shared) = producer.acquire().ok_or("no free buffer")?;
    let io = shared.io_surface().ok_or("not an IOSurface")?;
    io.write(|bytes, stride| {
        for row in bytes.chunks_mut(stride) {
            for pixel in row[..32].chunks_mut(4) {
                pixel.copy_from_slice(&[255, 0, 0, 255]);
            }
        }
    })?;
    producer.submit(Frame::new(buffer));
    let stats = stage.draw();
    assert_near(
        stage.image()?.pixel(50, 50),
        BLUE,
        "the producer's BGRA pixels",
    );
    assert_eq!(stats.image_bytes_uploaded, 0);
    Ok(())
}

fn filled<T: View>(
    view: impl FnOnce(SurfaceSource) -> T,
) -> Result<(TestSurface, TestUi), Box<dyn Error>> {
    let (mut producer, source) = TestSurface::new();
    let mut ui = TestUiBuilder::default()
        .size(100.0, 100.0)
        .clear(Color::WHITE)
        .build(|| view(source));
    ui.draw()?;
    producer.pump()?;
    Ok((producer, ui))
}

fn config(producer: &TestSurface) -> Result<(u32, u32, f32, bool), &'static str> {
    let SurfaceConfig {
        width,
        height,
        scale,
        visible,
        ..
    } = producer
        .config()
        .ok_or("the producer was never configured")?;
    Ok((width, height, scale, visible))
}

fn scene(builder: TestSurfaceBuilder) -> Result<(TestSurface, Stage), Box<dyn Error>> {
    let (mut surface, source) = builder.build();
    let mut stage = staged(source)?;
    stage.draw();
    surface.pump()?;
    Ok((surface, stage))
}

fn staged(source: SurfaceSource) -> Result<Stage, GpuError> {
    StageBuilder::default()
        .sized(100.0, 100.0)
        .surface(source)
        .build()
}

fn place(stage: &mut Stage, id: SurfaceId, visible: bool) {
    let config = SurfaceConfig {
        width: 100,
        height: 100,
        scale: 1.0,
        visible,
    };
    stage.surfaces().place(id, config);
}

mod common;

use common::{BLUE, CHILD, RED, ROOT, Stage, StageBuilder, TestResult, WHITE};
#[cfg(target_os = "macos")]
use rouge_interop::IoSurface;
#[cfg(target_os = "macos")]
use rouge_render::Sharing;
use rouge_render::headless::assert_near;
use rouge_render::testing::{Pattern, TestSurface, TestSurfaceBuilder, TestSurfaceError};
use rouge_render::{
    AlphaMode, BufferDesc, BufferRef, Color, ColorMatrix, Command, DrawList, EffectSlot, Frame,
    Glass, GpuError, LayerCommand, LayerId, LayerProp, Nested, Origin, Placement, QueueMode, Rect,
    SurfaceError, SurfaceEvent, SurfaceId, SurfaceOptions, SurfaceSource, SurfaceStatus,
    surface_channel,
};
use std::error::Error;
use std::time::{Duration, Instant};
use wgpu::{TexelCopyBufferLayout, TextureFormat};

const GREEN: [u8; 4] = [0, 255, 0, 255];
const QUADRANTS: Pattern = Pattern::quadrants([RED, GREEN, BLUE, WHITE]);

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
        stage.set(CHILD, prop, 30.0);
    }
    stage.set(CHILD, LayerProp::Scale, 0.5);
    stage.set(CHILD, LayerProp::Opacity, 0.5);
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
    stage.effect(CHILD, EffectSlot::Content(0), &ColorMatrix::IDENTITY, false);
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
    let glass = LayerId(2);
    stage.apply(Command::Create(glass));
    stage.layer(
        glass,
        LayerCommand::Frame(Rect::new(20.0, 20.0, 60.0, 60.0)),
    );
    stage.effect(glass, EffectSlot::Backdrop, &Glass::frosted(8.0), false);
    stage.content(glass, vec![DrawList::new()], vec![]);
    let nested = [CHILD, glass].map(|layer| Nested { layer, clip: None });
    stage.content(
        ROOT,
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
    let first = surface.present(Pattern::Solid(RED))?;
    surface.present(Pattern::Solid(BLUE))?;
    stage.draw();
    assert_near(stage.image()?.pixel(50, 50), BLUE, "the newest frame");
    let events = surface.pump()?.to_vec();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SurfaceEvent::Discarded(frame) if *frame == first))
    );
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
fn a_frame_waits_for_its_fence() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present(Pattern::Solid(RED))?;
    stage.draw();
    let (signal, fence) = surface.fence();
    surface.present_frame(Pattern::Solid(BLUE), |frame| frame.after(fence))?;
    assert!(!stage.surfaces().latch(Instant::now()));
    signal.signal();
    assert!(stage.surfaces().latch(Instant::now()));
    stage.draw();
    assert_near(stage.image()?.pixel(50, 50), BLUE, "after the fence");
    Ok(())
}

#[test]
fn timed_frames_are_shown_at_their_presentation_time() -> TestResult {
    let builder = TestSurface::builder()
        .size(100, 100)
        .queue(QueueMode::Timed);
    let (mut surface, mut stage) = scene(builder)?;
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    surface.present_frame(Pattern::Solid(RED), |frame| frame.present_at(at(10)))?;
    surface.present_frame(Pattern::Solid(GREEN), |frame| frame.present_at(at(20)))?;
    surface.present_frame(Pattern::Solid(BLUE), |frame| frame.present_at(at(30)))?;
    let surfaces = stage.surfaces();
    assert!(!surfaces.latch(at(5)), "nothing is due");
    assert_eq!(surfaces.next_deadline(), Some(at(10)));
    assert!(surfaces.latch(at(25)), "the latest due frame wins");
    assert_eq!(surfaces.stats().frames_discarded, 1);
    assert_eq!(surfaces.next_deadline(), Some(at(30)));
    stage.headless.render(
        Color::TRANSPARENT,
        &stage.compositor.scene(1.0),
        &mut stage.atlas,
    );
    assert_near(stage.image()?.pixel(50, 50), GREEN, "the frame due at 20ms");
    Ok(())
}

#[test]
fn frame_requests_are_answered_after_presenting_only_while_visible() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    let now = Instant::now();
    let interval = Duration::from_millis(8);
    surface.request_frame();
    stage.surfaces().latch(now);
    assert!(stage.surfaces().needs_frame());
    stage.surfaces().presented(now, interval);
    assert!(!stage.surfaces().needs_frame());
    let events = surface.pump()?.to_vec();
    assert!(events.iter().any(
        |event| matches!(event, SurfaceEvent::FrameRequest { target } if *target == now + interval)
    ));

    place(&mut stage, surface.id(), false);
    surface.request_frame();
    stage.surfaces().latch(now);
    assert!(
        !stage.surfaces().needs_frame(),
        "hidden surfaces are not paced"
    );
    Ok(())
}

#[test]
fn configuration_changes_are_numbered_and_sent_once() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder())?;
    assert_eq!(surface.config().map(|config| config.seq), Some(1));
    place(&mut stage, surface.id(), true);
    stage.surfaces().latch(Instant::now());
    assert!(
        surface.pump()?.is_empty(),
        "an unchanged placement is not resent"
    );
    place(&mut stage, surface.id(), false);
    stage.surfaces().latch(Instant::now());
    surface.pump()?;
    let config = surface.config().ok_or("no config")?;
    assert_eq!((config.seq, config.visible), (2, false));
    assert_eq!((config.width, config.height), (100, 100));
    Ok(())
}

#[test]
fn a_detached_surface_keeps_its_last_frame_until_its_layer_is_gone() -> TestResult {
    let (mut surface, mut stage) = scene(TestSurface::builder().size(100, 100))?;
    surface.present(Pattern::Solid(RED))?;
    stage.draw();
    stage.surfaces().registrar().detach(surface.id());
    stage.draw();
    surface.pump()?;
    assert!(surface.is_detached());
    assert_near(stage.image()?.pixel(50, 50), RED, "the last frame stays");
    assert_eq!(stage.surfaces().stats().surfaces, 1);

    stage.apply(Command::Remove(CHILD));
    stage.draw();
    stage.headless.wait()?;
    stage.surfaces().maintain();
    assert_eq!(stage.surfaces().stats().surfaces, 0);
    let events = surface.pump()?.to_vec();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, SurfaceEvent::Released(_)))
    );
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
    let (mut producer, source) = surface_channel(SurfaceOptions::default());
    let mut stage = staged(source)?;
    stage.draw();
    let Some(SurfaceEvent::Attached(cx)) = producer.try_recv() else {
        return Err("the producer was not attached".into());
    };
    assert!(matches!(
        producer.try_recv(),
        Some(SurfaceEvent::Configure(_))
    ));
    let buffers = cx.allocate(&BufferDesc::new(4, 4).count(1))?;
    let set = producer.register(&buffers);
    let pixels: Vec<u8> = [0u8, 0, 255, 255].repeat(16);
    cx.queue().write_texture(
        buffers[0].texture().as_image_copy(),
        &pixels,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: None,
        },
        buffers[0].texture().size(),
    );
    producer.submit(Frame::new(BufferRef { set, index: 0 }));
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
    let (mut producer, source) = surface_channel(SurfaceOptions::default());
    let mut stage = staged(source)?;
    stage.draw();
    let Some(SurfaceEvent::Attached(cx)) = producer.try_recv() else {
        return Err("the producer was not attached".into());
    };
    let io = IoSurface::new(8, 8)?;
    io.write(|bytes, stride| {
        for row in bytes.chunks_mut(stride) {
            for pixel in row[..32].chunks_mut(4) {
                pixel.copy_from_slice(&[255, 0, 0, 255]);
            }
        }
    })?;
    let buffer = cx.import_io_surface(io)?;
    let set = producer.register(&[buffer]);
    producer.submit(Frame::new(BufferRef { set, index: 0 }));
    let stats = stage.draw();
    assert_near(
        stage.image()?.pixel(50, 50),
        BLUE,
        "the producer's BGRA pixels",
    );
    assert_eq!(stats.image_bytes_uploaded, 0);
    Ok(())
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
    let placement = Placement {
        width: 100,
        height: 100,
        scale: 1.0,
        visible,
    };
    stage.surfaces().registrar().place(id, placement);
}

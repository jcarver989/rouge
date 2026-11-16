use rouge_core::{CompositorEvent, Layers, Message, Rect, WindowConfig, WindowId, mailbox, spawn};
use rouge_render::Target;
use rouge_render::headless::headless_gpu;
use std::error::Error;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

const WINDOW: WindowId = WindowId(3);
const WAIT: Duration = Duration::from_secs(5);

#[test]
fn commits_cross_to_the_compositor_thread_and_are_acknowledged() -> Result<(), Box<dyn Error>> {
    let gpu = headless_gpu()?;
    let (messages, mailbox, events) = mailbox();
    let (woken, wakes) = channel();
    let wake = move || {
        let _ = woken.send(());
    };
    let thread = spawn(gpu.clone(), messages.clone(), mailbox, Instant::now(), wake)?;
    let config = WindowConfig {
        width: 32,
        height: 32,
        interval: Duration::from_millis(8),
        visible: true,
    };
    let open = Message::Open(WINDOW, Target::offscreen(&gpu, 32, 32), config);
    messages.send(open).map_err(|_| "the thread is gone")?;
    let mut layers = Layers::new();
    let root = layers.create_layer();
    layers.set_root(root);
    layers.set_frame(root, Rect::new(0.0, 0.0, 32.0, 32.0));
    let commit = Message::Commit(WINDOW, layers.commit());
    messages.send(commit).map_err(|_| "the thread is gone")?;

    wakes.recv_timeout(WAIT)?;
    assert_eq!(
        events.recv_timeout(WAIT)?,
        (WINDOW, CompositorEvent::Presented)
    );
    (messages.send(Message::Shutdown)).map_err(|_| "the thread is gone")?;
    let targets = thread.join().map_err(|_| "the compositor panicked")?;
    assert_eq!(targets.len(), 1, "targets come back to the caller's thread");
    Ok(())
}

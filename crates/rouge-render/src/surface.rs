use crate::gpu::create_texture;
#[cfg(target_os = "macos")]
use crate::interop::{IoSurface, import_io_surface};
use crate::{Gpu, Rect};
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use std::mem::take;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};
#[cfg(not(target_arch = "wasm32"))]
use wgpu::PollType;
use wgpu::{AdapterInfo, Device, Queue, Texture, TextureFormat, TextureUsages};

const RELEASE_POLL: Duration = Duration::from_millis(2);

static NEXT_SURFACE: AtomicU64 = AtomicU64::new(1);

type Waker = Arc<dyn Fn() + Send + Sync>;
type StatusSink = Box<dyn Fn(SurfaceStatus)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SurfaceId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferRef {
    pub(crate) set: u64,
    index: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    #[default]
    TopLeft,
    BottomLeft,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    #[default]
    Premultiplied,
    Opaque,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    buffer: BufferRef,
    region: Option<Rect>,
    origin: Origin,
    alpha: AlphaMode,
}

impl Frame {
    pub fn new(buffer: BufferRef) -> Self {
        Frame {
            buffer,
            region: None,
            origin: Origin::TopLeft,
            alpha: AlphaMode::Premultiplied,
        }
    }

    pub fn region(mut self, region: Rect) -> Self {
        self.region = Some(region);
        self
    }

    pub fn origin(mut self, origin: Origin) -> Self {
        self.origin = origin;
        self
    }

    pub fn alpha(mut self, alpha: AlphaMode) -> Self {
        self.alpha = alpha;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceConfig {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub visible: bool,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SurfaceError {
    #[error("this platform cannot share {0} with the compositor")]
    Unsupported(&'static str),
    #[error("the surface has not been attached to a compositor")]
    NotAttached,
    #[error("the producer renders on a different GPU from the compositor")]
    AdapterMismatch,
    #[error("surface buffers must be BGRA8 or RGBA8 without sRGB encoding, not {0:?}")]
    Format(TextureFormat),
    #[error("failed to allocate a {width}x{height} shared buffer")]
    Allocation { width: u32, height: u32 },
    #[error("failed to share a buffer with the compositor: {0}")]
    Import(String),
    #[error("{0}")]
    Producer(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SurfaceStatus {
    #[default]
    Waiting,
    Live,
    Failed(SurfaceError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceEvent {
    Attached,
    Configure(SurfaceConfig),
    FrameRequest { target: Instant },
    Detached,
}

enum Notice {
    Attached(Box<SurfaceCx>),
    Event(SurfaceEvent),
    Released(BufferRef),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sharing {
    #[default]
    Device,
    Native,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferDesc {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TextureFormat,
    pub(crate) count: u32,
    pub(crate) sharing: Sharing,
}

impl BufferDesc {
    pub fn new(width: u32, height: u32) -> Self {
        BufferDesc {
            width,
            height,
            format: TextureFormat::Bgra8Unorm,
            count: 3,
            sharing: Sharing::Device,
        }
    }

    pub fn format(mut self, format: TextureFormat) -> Self {
        self.format = format;
        self
    }

    pub fn count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }

    pub fn sharing(mut self, sharing: Sharing) -> Self {
        self.sharing = sharing;
        self
    }
}

#[derive(Clone, Debug)]
pub struct SharedBuffer {
    texture: Texture,
    #[cfg(target_os = "macos")]
    io_surface: Option<IoSurface>,
}

impl SharedBuffer {
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    #[cfg(target_os = "macos")]
    pub fn io_surface(&self) -> Option<&IoSurface> {
        self.io_surface.as_ref()
    }
}

#[derive(Clone)]
pub struct SurfaceCx {
    device: Device,
    queue: Queue,
    adapter: AdapterInfo,
    waker: Option<Waker>,
}

impl SurfaceCx {
    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn adapter(&self) -> &AdapterInfo {
        &self.adapter
    }

    fn allocate(&self, desc: &BufferDesc) -> Result<Vec<SharedBuffer>, SurfaceError> {
        check_format(desc.format)?;
        (0..desc.count.max(1))
            .map(|_| match desc.sharing {
                Sharing::Device => Ok(SharedBuffer {
                    texture: create_texture(
                        &self.device,
                        "rouge surface buffer",
                        (desc.width.max(1), desc.height.max(1)),
                        desc.format,
                        buffer_usage(),
                    ),
                    #[cfg(target_os = "macos")]
                    io_surface: None,
                }),
                Sharing::Native => native_buffer(self, desc),
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
fn native_buffer(cx: &SurfaceCx, desc: &BufferDesc) -> Result<SharedBuffer, SurfaceError> {
    if desc.format != TextureFormat::Bgra8Unorm {
        return Err(SurfaceError::Format(desc.format));
    }
    let surface = IoSurface::new(desc.width, desc.height)?;
    let texture = import_io_surface(&cx.device, &surface, desc.format, buffer_usage())?;
    Ok(SharedBuffer {
        texture,
        io_surface: Some(surface),
    })
}

#[cfg(not(target_os = "macos"))]
fn native_buffer(_cx: &SurfaceCx, _desc: &BufferDesc) -> Result<SharedBuffer, SurfaceError> {
    Err(SurfaceError::Unsupported("native buffers"))
}

fn buffer_usage() -> TextureUsages {
    TextureUsages::TEXTURE_BINDING
        | TextureUsages::RENDER_ATTACHMENT
        | TextureUsages::COPY_SRC
        | TextureUsages::COPY_DST
}

fn check_format(format: TextureFormat) -> Result<(), SurfaceError> {
    match format {
        TextureFormat::Bgra8Unorm | TextureFormat::Rgba8Unorm => Ok(()),
        other => Err(SurfaceError::Format(other)),
    }
}

enum Message {
    Register(u64, Vec<Texture>),
    Unregister(u64),
    Submit(Frame),
    RequestFrame,
    Fail(SurfaceError),
}

struct Pool {
    desc: BufferDesc,
    set: u64,
    buffers: Vec<SharedBuffer>,
    free: VecDeque<u32>,
}

pub struct SurfaceProducer {
    id: SurfaceId,
    messages: UnboundedSender<Message>,
    notices: UnboundedReceiver<Notice>,
    cx: Option<SurfaceCx>,
    pool: Option<Pool>,
    sets: u64,
}

pub struct SurfaceSource {
    id: SurfaceId,
    messages: UnboundedReceiver<Message>,
    notices: UnboundedSender<Notice>,
}

impl SurfaceSource {
    pub fn id(&self) -> SurfaceId {
        self.id
    }
}

pub fn surface_channel() -> (SurfaceProducer, SurfaceSource) {
    let id = SurfaceId(NEXT_SURFACE.fetch_add(1, Ordering::Relaxed));
    let (messages, inbox) = unbounded_channel();
    let (notices, outbox) = unbounded_channel();
    let producer = SurfaceProducer {
        id,
        messages,
        notices: outbox,
        cx: None,
        pool: None,
        sets: 0,
    };
    let source = SurfaceSource {
        id,
        messages: inbox,
        notices,
    };
    (producer, source)
}

impl SurfaceProducer {
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    pub fn cx(&self) -> Option<&SurfaceCx> {
        self.cx.as_ref()
    }

    pub fn configure(&mut self, desc: BufferDesc) -> Result<(), SurfaceError> {
        if self.pool.as_ref().is_some_and(|pool| pool.desc == desc) {
            return Ok(());
        }
        let cx = self.cx.as_ref().ok_or(SurfaceError::NotAttached)?;
        let buffers = cx.allocate(&desc)?;
        self.sets += 1;
        let textures = buffers
            .iter()
            .map(|buffer| buffer.texture.clone())
            .collect();
        self.send(Message::Register(self.sets, textures));
        if let Some(old) = self.pool.take() {
            self.send(Message::Unregister(old.set));
        }
        self.pool = Some(Pool {
            desc,
            set: self.sets,
            free: (0..buffers.len() as u32).collect(),
            buffers,
        });
        Ok(())
    }

    pub fn acquire(&mut self) -> Option<(BufferRef, &SharedBuffer)> {
        let pool = self.pool.as_mut()?;
        let index = pool.free.pop_front()?;
        let buffer = BufferRef {
            set: pool.set,
            index,
        };
        Some((buffer, &pool.buffers[index as usize]))
    }

    pub fn buffers(&self) -> &[SharedBuffer] {
        self.pool.as_ref().map_or(&[], |pool| &pool.buffers)
    }

    pub fn free_buffers(&self) -> usize {
        self.pool.as_ref().map_or(0, |pool| pool.free.len())
    }

    pub fn submit(&mut self, frame: Frame) {
        self.send(Message::Submit(frame));
    }

    pub fn request_frame(&self) {
        self.send(Message::RequestFrame);
    }

    pub fn fail(&self, error: SurfaceError) {
        self.send(Message::Fail(error));
    }

    pub fn try_recv(&mut self) -> Option<SurfaceEvent> {
        while let Ok(notice) = self.notices.try_recv() {
            if let Some(event) = self.observe(notice) {
                return Some(event);
            }
        }
        None
    }

    pub async fn recv(&mut self) -> Option<SurfaceEvent> {
        loop {
            let notice = self.notices.recv().await?;
            if let Some(event) = self.observe(notice) {
                return Some(event);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn blocking_recv(&mut self) -> Option<SurfaceEvent> {
        loop {
            let notice = self.notices.blocking_recv()?;
            if let Some(event) = self.observe(notice) {
                return Some(event);
            }
        }
    }

    fn observe(&mut self, notice: Notice) -> Option<SurfaceEvent> {
        match notice {
            Notice::Attached(cx) => {
                self.cx = Some(*cx);
                Some(SurfaceEvent::Attached)
            }
            Notice::Event(event) => Some(event),
            Notice::Released(buffer) => {
                if let Some(pool) = &mut self.pool
                    && pool.set == buffer.set
                    && !pool.free.contains(&buffer.index)
                {
                    pool.free.push_back(buffer.index);
                }
                None
            }
        }
    }

    fn send(&self, message: Message) {
        if self.messages.send(message).is_ok()
            && let Some(waker) = self.cx.as_ref().and_then(|cx| cx.waker.as_ref())
        {
            waker();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceStats {
    pub surfaces: usize,
    pub frames_submitted: u64,
    pub frames_latched: u64,
    pub frames_discarded: u64,
    pub buffers_released: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct Latched {
    pub buffer: BufferRef,
    pub texture: Texture,
    pub region: Rect,
    pub origin: Origin,
    pub alpha: AlphaMode,
}

struct Entry {
    messages: UnboundedReceiver<Message>,
    notices: UnboundedSender<Notice>,
    status: StatusSink,
    sets: Vec<(u64, Vec<Texture>)>,
    pending: Option<Frame>,
    current: Option<Latched>,
    latched: u64,
    config: Option<SurfaceConfig>,
    binds: usize,
    wants_frame: bool,
    detached: bool,
    failed: bool,
}

impl Entry {
    fn visible(&self) -> bool {
        !self.detached && self.config.is_some_and(|config| config.visible)
    }

    fn emit(&self, event: SurfaceEvent) {
        let _ = self.notices.send(Notice::Event(event));
    }

    fn texture(&self, buffer: BufferRef) -> Option<&Texture> {
        let (_, textures) = self.sets.iter().find(|(set, _)| *set == buffer.set)?;
        textures.get(buffer.index as usize)
    }

    fn discard(&self, frame: Frame, stats: &mut SurfaceStats) {
        let _ = self.notices.send(Notice::Released(frame.buffer));
        stats.frames_discarded += 1;
    }

    fn fail(&mut self, error: SurfaceError, stats: &mut SurfaceStats) {
        self.failed = true;
        if let Some(frame) = self.pending.take() {
            self.discard(frame, stats);
        }
        (self.status)(SurfaceStatus::Failed(error));
    }

    fn receive(&mut self, stats: &mut SurfaceStats) {
        while let Ok(message) = self.messages.try_recv() {
            match message {
                Message::Register(set, textures) => {
                    let invalid = textures
                        .iter()
                        .map(Texture::format)
                        .find(|format| check_format(*format).is_err());
                    match invalid {
                        Some(format) => self.fail(SurfaceError::Format(format), stats),
                        None => self.sets.push((set, textures)),
                    }
                }
                Message::Unregister(set) => self.sets.retain(|(id, _)| *id != set),
                Message::Submit(frame) => {
                    stats.frames_submitted += 1;
                    if self.detached || self.failed || self.texture(frame.buffer).is_none() {
                        self.discard(frame, stats);
                    } else if let Some(skipped) = self.pending.replace(frame) {
                        self.discard(skipped, stats);
                    }
                }
                Message::RequestFrame => self.wants_frame = true,
                Message::Fail(error) => self.fail(error, stats),
            }
        }
    }

    fn latch(&mut self, retired: &mut Vec<Retired>, stats: &mut SurfaceStats) -> bool {
        self.receive(stats);
        let Some(frame) = self.pending.take() else {
            return false;
        };
        let Some(texture) = self.texture(frame.buffer).cloned() else {
            self.discard(frame, stats);
            return false;
        };
        let full = Rect::new(0.0, 0.0, texture.width() as f32, texture.height() as f32);
        let latched = Latched {
            buffer: frame.buffer,
            texture,
            region: frame.region.map_or(full, |region| region.intersect(&full)),
            origin: frame.origin,
            alpha: frame.alpha,
        };
        if let Some(previous) = self.current.replace(latched) {
            retired.push(Retired {
                notices: self.notices.clone(),
                buffer: previous.buffer,
            });
        } else {
            (self.status)(SurfaceStatus::Live);
        }
        self.latched += 1;
        stats.frames_latched += 1;
        true
    }
}

struct Retired {
    notices: UnboundedSender<Notice>,
    buffer: BufferRef,
}

impl Retired {
    fn release(self) {
        let _ = self.notices.send(Notice::Released(self.buffer));
    }
}

pub struct Surfaces {
    device: Device,
    queue: Queue,
    adapter: AdapterInfo,
    waker: Option<Waker>,
    entries: FxHashMap<SurfaceId, Entry>,
    retired: Vec<Retired>,
    outstanding: Arc<AtomicUsize>,
    stats: SurfaceStats,
}

impl Surfaces {
    pub fn new(gpu: &Gpu) -> Self {
        Surfaces {
            device: gpu.device.clone(),
            queue: gpu.queue.clone(),
            adapter: gpu.adapter.get_info(),
            waker: None,
            entries: FxHashMap::default(),
            retired: Vec::new(),
            outstanding: Arc::new(AtomicUsize::new(0)),
            stats: SurfaceStats::default(),
        }
    }

    pub fn with_waker(mut self, waker: impl Fn() + Send + Sync + 'static) -> Self {
        self.waker = Some(Arc::new(waker));
        self
    }

    pub fn stats(&self) -> SurfaceStats {
        SurfaceStats {
            surfaces: self.entries.len(),
            ..self.stats
        }
    }

    pub fn latch(&mut self) -> bool {
        let mut changed = false;
        for entry in self.entries.values_mut() {
            changed |= entry.latch(&mut self.retired, &mut self.stats);
        }
        self.collect();
        changed
    }

    pub fn needs_frame(&self) -> bool {
        self.entries
            .values()
            .any(|entry| entry.wants_frame && entry.visible())
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        (self.outstanding.load(Ordering::Acquire) > 0).then(|| Instant::now() + RELEASE_POLL)
    }

    pub fn presented(&mut self, at: Instant, interval: Duration) {
        for entry in self.entries.values_mut() {
            if entry.wants_frame && entry.visible() {
                entry.wants_frame = false;
                entry.emit(SurfaceEvent::FrameRequest {
                    target: at + interval,
                });
            }
        }
    }

    pub fn maintain(&mut self) {
        if !self.retired.is_empty() {
            self.stats.buffers_released += self.retired.len() as u64;
            #[cfg(not(target_arch = "wasm32"))]
            self.release_after_gpu();
            #[cfg(target_arch = "wasm32")]
            self.retired.drain(..).for_each(Retired::release);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.outstanding.load(Ordering::Acquire) > 0 {
            let _ = self.device.poll(PollType::Poll);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn release_after_gpu(&mut self) {
        let retired = take(&mut self.retired);
        self.outstanding.fetch_add(1, Ordering::AcqRel);
        let outstanding = self.outstanding.clone();
        self.queue.on_submitted_work_done(move || {
            retired.into_iter().for_each(Retired::release);
            outstanding.fetch_sub(1, Ordering::AcqRel);
        });
    }

    pub fn frame(&self, id: SurfaceId) -> Option<u64> {
        let entry = self.entries.get(&id)?;
        entry.current.as_ref().map(|_| entry.latched)
    }

    pub(crate) fn latched(&self, id: SurfaceId) -> Option<&Latched> {
        self.entries.get(&id)?.current.as_ref()
    }

    pub(crate) fn holds(&self, id: SurfaceId, set: u64) -> bool {
        self.entries.get(&id).is_some_and(|entry| {
            entry.sets.iter().any(|(held, _)| *held == set)
                || entry
                    .current
                    .as_ref()
                    .is_some_and(|current| current.buffer.set == set)
        })
    }

    pub fn place(&mut self, id: SurfaceId, config: SurfaceConfig) {
        if let Some(entry) = self.entries.get_mut(&id)
            && entry.config != Some(config)
            && !entry.detached
        {
            entry.config = Some(config);
            entry.emit(SurfaceEvent::Configure(config));
        }
    }

    pub fn detach(&mut self, id: SurfaceId) {
        if let Some(entry) = self.entries.get_mut(&id)
            && !entry.detached
        {
            entry.detached = true;
            entry.wants_frame = false;
            if let Some(frame) = entry.pending.take() {
                entry.discard(frame, &mut self.stats);
            }
            entry.emit(SurfaceEvent::Detached);
        }
    }

    pub fn bind(&mut self, id: SurfaceId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.binds += 1;
        }
    }

    pub fn unbind(&mut self, id: SurfaceId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.binds = entry.binds.saturating_sub(1);
        }
    }

    pub fn attach(&mut self, source: SurfaceSource, status: impl Fn(SurfaceStatus) + 'static) {
        let SurfaceSource {
            id,
            messages,
            notices,
        } = source;
        let cx = SurfaceCx {
            device: self.device.clone(),
            queue: self.queue.clone(),
            adapter: self.adapter.clone(),
            waker: self.waker.clone(),
        };
        let _ = notices.send(Notice::Attached(Box::new(cx)));
        status(SurfaceStatus::Waiting);
        self.entries.insert(
            id,
            Entry {
                messages,
                notices,
                status: Box::new(status),
                sets: Vec::new(),
                pending: None,
                current: None,
                latched: 0,
                config: None,
                binds: 0,
                wants_frame: false,
                detached: false,
                failed: false,
            },
        );
    }

    fn collect(&mut self) {
        let retired = &mut self.retired;
        self.entries.retain(|_, entry| {
            let keep = !entry.detached || entry.binds > 0;
            if !keep && let Some(current) = entry.current.take() {
                retired.push(Retired {
                    notices: entry.notices.clone(),
                    buffer: current.buffer,
                });
            }
            keep
        });
    }
}

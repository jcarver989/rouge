use crate::{Gpu, Rect};
#[cfg(target_os = "macos")]
use rouge_interop::{InteropError, IoSurface, import_io_surface};
use std::collections::{HashMap, VecDeque};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::mem::take;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot::{self, error::TryRecvError as FenceError};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};
#[cfg(not(target_arch = "wasm32"))]
use wgpu::PollType;
use wgpu::{
    AdapterInfo, Device, Extent3d, Queue, Texture, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages,
};

const RELEASE_POLL: Duration = Duration::from_millis(2);

static NEXT_SURFACE: AtomicU64 = AtomicU64::new(1);

type Waker = Arc<dyn Fn() + Send + Sync>;
type StatusSink = Box<dyn Fn(SurfaceStatus)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SurfaceId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BufferSetId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferRef {
    pub set: BufferSetId,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameId(u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueueMode {
    #[default]
    Mailbox,
    Timed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceOptions {
    queue: QueueMode,
}

impl SurfaceOptions {
    pub fn queue(mut self, queue: QueueMode) -> Self {
        self.queue = queue;
        self
    }
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

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorSpace {
    #[default]
    Srgb,
}

pub struct Fence(oneshot::Receiver<()>);

pub struct FenceSignal {
    sender: oneshot::Sender<()>,
    waker: Option<Waker>,
}

impl FenceSignal {
    pub fn signal(self) {
        let _ = self.sender.send(());
        if let Some(waker) = &self.waker {
            waker();
        }
    }
}

#[derive(Default)]
pub enum Acquire {
    #[default]
    Ready,
    Fence(Fence),
}

pub struct Frame {
    buffer: BufferRef,
    region: Option<Rect>,
    origin: Origin,
    alpha: AlphaMode,
    color: ColorSpace,
    damage: Option<Rect>,
    acquire: Acquire,
    present_at: Option<Instant>,
    config: u64,
}

impl Frame {
    pub fn new(buffer: BufferRef) -> Self {
        Frame {
            buffer,
            region: None,
            origin: Origin::TopLeft,
            alpha: AlphaMode::Premultiplied,
            color: ColorSpace::Srgb,
            damage: None,
            acquire: Acquire::Ready,
            present_at: None,
            config: 0,
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

    pub fn color(mut self, color: ColorSpace) -> Self {
        self.color = color;
        self
    }

    pub fn damage(mut self, damage: Rect) -> Self {
        self.damage = Some(damage);
        self
    }

    pub fn after(mut self, fence: Fence) -> Self {
        self.acquire = Acquire::Fence(fence);
        self
    }

    pub fn present_at(mut self, at: Instant) -> Self {
        self.present_at = Some(at);
        self
    }

    pub fn config(mut self, seq: u64) -> Self {
        self.config = seq;
        self
    }

    pub fn buffer(&self) -> BufferRef {
        self.buffer
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceConfig {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub visible: bool,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SurfaceError {
    #[error("this platform cannot share {0} with the compositor")]
    Unsupported(&'static str),
    #[error("the producer renders on a different GPU from the compositor")]
    AdapterMismatch,
    #[error("surface buffers must be BGRA8 or RGBA8 without sRGB encoding, not {0:?}")]
    Format(TextureFormat),
    #[error("failed to share a buffer with the compositor: {0}")]
    Import(String),
    #[error("{0}")]
    Producer(String),
}

#[cfg(target_os = "macos")]
impl From<InteropError> for SurfaceError {
    fn from(error: InteropError) -> Self {
        match error {
            InteropError::Unsupported(what) => SurfaceError::Unsupported(what),
            InteropError::Format { format, .. } => SurfaceError::Format(format),
            other => SurfaceError::Import(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SurfaceStatus {
    #[default]
    Waiting,
    Live,
    Failed(SurfaceError),
}

#[derive(Clone)]
pub enum SurfaceEvent {
    Attached(Box<SurfaceCx>),
    Configure(SurfaceConfig),
    FrameRequest { target: Instant },
    Released(BufferRef),
    Presented { frame: FrameId, at: Instant },
    Discarded(FrameId),
    Detached,
}

impl Debug for SurfaceEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            SurfaceEvent::Attached(cx) => write!(f, "Attached({})", cx.adapter.name),
            SurfaceEvent::Configure(config) => write!(f, "Configure({config:?})"),
            SurfaceEvent::FrameRequest { target } => write!(f, "FrameRequest({target:?})"),
            SurfaceEvent::Released(buffer) => write!(f, "Released({buffer:?})"),
            SurfaceEvent::Presented { frame, at } => write!(f, "Presented({frame:?}, {at:?})"),
            SurfaceEvent::Discarded(frame) => write!(f, "Discarded({frame:?})"),
            SurfaceEvent::Detached => write!(f, "Detached"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sharing {
    #[default]
    Device,
    Native,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferDesc {
    width: u32,
    height: u32,
    format: TextureFormat,
    count: u32,
    sharing: Sharing,
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
pub enum NativeBuffer {
    #[cfg(target_os = "macos")]
    IoSurface(IoSurface),
}

#[derive(Clone, Debug)]
pub struct SharedBuffer {
    texture: Texture,
    native: Option<NativeBuffer>,
}

impl SharedBuffer {
    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn native(&self) -> Option<&NativeBuffer> {
        self.native.as_ref()
    }

    #[cfg(target_os = "macos")]
    pub fn io_surface(&self) -> Option<&IoSurface> {
        match &self.native {
            Some(NativeBuffer::IoSurface(surface)) => Some(surface),
            None => None,
        }
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

    pub fn allocate(&self, desc: &BufferDesc) -> Result<Vec<SharedBuffer>, SurfaceError> {
        check_format(desc.format)?;
        (0..desc.count.max(1))
            .map(|_| match desc.sharing {
                Sharing::Device => Ok(SharedBuffer {
                    texture: self.device_texture(desc),
                    native: None,
                }),
                Sharing::Native => native_buffer(self, desc),
            })
            .collect()
    }

    #[cfg(target_os = "macos")]
    pub fn import_io_surface(&self, surface: IoSurface) -> Result<SharedBuffer, SurfaceError> {
        let texture = import_io_surface(
            &self.device,
            &surface,
            TextureFormat::Bgra8Unorm,
            buffer_usage(),
        )?;
        Ok(SharedBuffer {
            texture,
            native: Some(NativeBuffer::IoSurface(surface)),
        })
    }

    fn device_texture(&self, desc: &BufferDesc) -> Texture {
        self.device.create_texture(&TextureDescriptor {
            label: Some("rouge surface buffer"),
            size: Extent3d {
                width: desc.width.max(1),
                height: desc.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: desc.format,
            usage: buffer_usage(),
            view_formats: &[],
        })
    }
}

#[cfg(target_os = "macos")]
fn native_buffer(cx: &SurfaceCx, desc: &BufferDesc) -> Result<SharedBuffer, SurfaceError> {
    if desc.format != TextureFormat::Bgra8Unorm {
        return Err(SurfaceError::Format(desc.format));
    }
    cx.import_io_surface(IoSurface::new(desc.width, desc.height)?)
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
    Register(BufferSetId, Vec<Texture>),
    Unregister(BufferSetId),
    Submit(FrameId, Frame),
    RequestFrame,
    Fail(SurfaceError),
}

pub struct SurfaceProducer {
    id: SurfaceId,
    messages: UnboundedSender<Message>,
    events: UnboundedReceiver<SurfaceEvent>,
    waker: Option<Waker>,
    next_set: u64,
    next_frame: u64,
}

pub struct SurfaceSource {
    id: SurfaceId,
    options: SurfaceOptions,
    messages: UnboundedReceiver<Message>,
    events: UnboundedSender<SurfaceEvent>,
}

impl SurfaceSource {
    pub fn id(&self) -> SurfaceId {
        self.id
    }
}

pub fn surface_channel(options: SurfaceOptions) -> (SurfaceProducer, SurfaceSource) {
    let id = SurfaceId(NEXT_SURFACE.fetch_add(1, Ordering::Relaxed));
    let (messages, inbox) = unbounded_channel();
    let (events, outbox) = unbounded_channel();
    let producer = SurfaceProducer {
        id,
        messages,
        events: outbox,
        waker: None,
        next_set: 0,
        next_frame: 0,
    };
    let source = SurfaceSource {
        id,
        options,
        messages: inbox,
        events,
    };
    (producer, source)
}

impl SurfaceProducer {
    pub fn id(&self) -> SurfaceId {
        self.id
    }

    pub fn register(&mut self, buffers: &[SharedBuffer]) -> BufferSetId {
        self.next_set += 1;
        let set = BufferSetId(self.next_set);
        let textures = buffers
            .iter()
            .map(|buffer| buffer.texture.clone())
            .collect();
        self.send(Message::Register(set, textures));
        set
    }

    pub fn unregister(&mut self, set: BufferSetId) {
        self.send(Message::Unregister(set));
    }

    pub fn submit(&mut self, frame: Frame) -> FrameId {
        self.next_frame += 1;
        let id = FrameId(self.next_frame);
        self.send(Message::Submit(id, frame));
        id
    }

    pub fn request_frame(&self) {
        self.send(Message::RequestFrame);
    }

    pub fn fail(&self, error: SurfaceError) {
        self.send(Message::Fail(error));
    }

    pub fn fence(&self) -> (FenceSignal, Fence) {
        let (sender, receiver) = oneshot::channel();
        let signal = FenceSignal {
            sender,
            waker: self.waker.clone(),
        };
        (signal, Fence(receiver))
    }

    pub fn try_recv(&mut self) -> Option<SurfaceEvent> {
        let event = self.events.try_recv().ok()?;
        Some(self.observe(event))
    }

    pub async fn recv(&mut self) -> Option<SurfaceEvent> {
        let event = self.events.recv().await?;
        Some(self.observe(event))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn blocking_recv(&mut self) -> Option<SurfaceEvent> {
        let event = self.events.blocking_recv()?;
        Some(self.observe(event))
    }

    fn observe(&mut self, event: SurfaceEvent) -> SurfaceEvent {
        if let SurfaceEvent::Attached(cx) = &event {
            self.waker.clone_from(&cx.waker);
        }
        event
    }

    fn send(&self, message: Message) {
        if self.messages.send(message).is_ok()
            && let Some(waker) = &self.waker
        {
            waker();
        }
    }
}

enum Op {
    Attach(SurfaceSource, StatusSink),
    Place(SurfaceId, Placement),
    Detach(SurfaceId),
    Bind(SurfaceId),
    Unbind(SurfaceId),
}

#[derive(Clone)]
pub struct SurfaceRegistrar(UnboundedSender<Op>);

impl SurfaceRegistrar {
    pub fn attach(&self, source: SurfaceSource, status: impl Fn(SurfaceStatus) + 'static) {
        let _ = self.0.send(Op::Attach(source, Box::new(status)));
    }

    pub fn place(&self, id: SurfaceId, placement: Placement) {
        let _ = self.0.send(Op::Place(id, placement));
    }

    pub fn detach(&self, id: SurfaceId) {
        let _ = self.0.send(Op::Detach(id));
    }

    pub fn bind(&self, id: SurfaceId) {
        let _ = self.0.send(Op::Bind(id));
    }

    pub fn unbind(&self, id: SurfaceId) {
        let _ = self.0.send(Op::Unbind(id));
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
    pub frame: FrameId,
    pub buffer: BufferRef,
    pub texture: Texture,
    pub region: Rect,
    pub origin: Origin,
    pub alpha: AlphaMode,
}

struct Pending {
    id: FrameId,
    frame: Frame,
    ready: bool,
}

struct Entry {
    options: SurfaceOptions,
    messages: UnboundedReceiver<Message>,
    events: UnboundedSender<SurfaceEvent>,
    status: StatusSink,
    sets: HashMap<BufferSetId, Vec<Texture>>,
    pending: VecDeque<Pending>,
    current: Option<Latched>,
    unreported: Vec<FrameId>,
    placement: Option<Placement>,
    seq: u64,
    wants_frame: bool,
    detached: bool,
    failed: bool,
}

impl Entry {
    fn visible(&self) -> bool {
        !self.detached && self.placement.is_some_and(|placement| placement.visible)
    }

    fn emit(&self, event: SurfaceEvent) {
        let _ = self.events.send(event);
    }

    fn discard(&self, pending: Pending, stats: &mut SurfaceStats) {
        self.emit(SurfaceEvent::Released(pending.frame.buffer));
        self.emit(SurfaceEvent::Discarded(pending.id));
        stats.frames_discarded += 1;
    }

    fn fail(&mut self, error: SurfaceError, stats: &mut SurfaceStats) {
        self.failed = true;
        for pending in take(&mut self.pending) {
            self.discard(pending, stats);
        }
        (self.status)(SurfaceStatus::Failed(error));
    }

    fn receive(&mut self, stats: &mut SurfaceStats) {
        loop {
            let message = match self.messages.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
            };
            match message {
                Message::Register(set, textures) => {
                    let invalid = textures
                        .iter()
                        .map(Texture::format)
                        .find(|format| check_format(*format).is_err());
                    match invalid {
                        Some(format) => self.fail(SurfaceError::Format(format), stats),
                        None => {
                            self.sets.insert(set, textures);
                        }
                    }
                }
                Message::Unregister(set) => {
                    self.sets.remove(&set);
                }
                Message::Submit(id, frame) => {
                    stats.frames_submitted += 1;
                    let known = self
                        .sets
                        .get(&frame.buffer.set)
                        .is_some_and(|set| (frame.buffer.index as usize) < set.len());
                    let pending = Pending {
                        id,
                        ready: matches!(frame.acquire, Acquire::Ready),
                        frame,
                    };
                    if self.detached || self.failed || !known {
                        self.discard(pending, stats);
                    } else {
                        self.pending.push_back(pending);
                    }
                }
                Message::RequestFrame => self.wants_frame = true,
                Message::Fail(error) => self.fail(error, stats),
            }
        }
    }

    fn poll_fences(&mut self, stats: &mut SurfaceStats) {
        let mut abandoned = Vec::new();
        for (index, pending) in self.pending.iter_mut().enumerate() {
            if let Acquire::Fence(fence) = &mut pending.frame.acquire {
                match fence.0.try_recv() {
                    Ok(()) => pending.ready = true,
                    Err(FenceError::Empty) => {}
                    Err(FenceError::Closed) => abandoned.push(index),
                }
            }
        }
        for index in abandoned.into_iter().rev() {
            if let Some(pending) = self.pending.remove(index) {
                self.discard(pending, stats);
            }
        }
    }

    fn choose(&self, target: Instant) -> Option<usize> {
        let due = |pending: &Pending| match self.options.queue {
            QueueMode::Mailbox => true,
            QueueMode::Timed => pending.frame.present_at.is_none_or(|at| at <= target),
        };
        self.pending
            .iter()
            .rposition(|pending| pending.ready && due(pending))
    }

    fn latch(
        &mut self,
        target: Instant,
        retired: &mut Vec<Retired>,
        stats: &mut SurfaceStats,
    ) -> bool {
        self.receive(stats);
        self.poll_fences(stats);
        let Some(chosen) = self.choose(target) else {
            return false;
        };
        let skipped: Vec<_> = self.pending.drain(..chosen).collect();
        for pending in skipped {
            self.discard(pending, stats);
        }
        let Some(pending) = self.pending.pop_front() else {
            return false;
        };
        let buffer = pending.frame.buffer;
        let Some(texture) = self
            .sets
            .get(&buffer.set)
            .and_then(|set| set.get(buffer.index as usize))
            .cloned()
        else {
            self.discard(pending, stats);
            return false;
        };
        let Pending { id, frame, .. } = pending;
        let full = Rect::new(0.0, 0.0, texture.width() as f32, texture.height() as f32);
        let region = frame.region.map_or(full, |region| region.intersect(&full));
        let latched = Latched {
            frame: id,
            buffer: frame.buffer,
            texture,
            region,
            origin: frame.origin,
            alpha: frame.alpha,
        };
        if let Some(previous) = self.current.replace(latched) {
            retired.push(Retired {
                events: self.events.clone(),
                buffer: previous.buffer,
            });
        } else {
            (self.status)(SurfaceStatus::Live);
        }
        self.unreported.push(id);
        stats.frames_latched += 1;
        true
    }
}

struct Retired {
    events: UnboundedSender<SurfaceEvent>,
    buffer: BufferRef,
}

#[cfg(target_arch = "wasm32")]
fn release(retired: Vec<Retired>) {
    for Retired { events, buffer } in retired {
        let _ = events.send(SurfaceEvent::Released(buffer));
    }
}

pub struct Surfaces {
    device: Device,
    queue: Queue,
    adapter: AdapterInfo,
    waker: Option<Waker>,
    ops: UnboundedReceiver<Op>,
    registrar: UnboundedSender<Op>,
    entries: HashMap<SurfaceId, Entry>,
    binds: HashMap<SurfaceId, usize>,
    retired: Vec<Retired>,
    outstanding: Arc<AtomicUsize>,
    stats: SurfaceStats,
}

impl Surfaces {
    pub fn new(gpu: &Gpu) -> Self {
        let (registrar, ops) = unbounded_channel();
        Surfaces {
            device: gpu.device.clone(),
            queue: gpu.queue.clone(),
            adapter: gpu.adapter.get_info(),
            waker: None,
            ops,
            registrar,
            entries: HashMap::new(),
            binds: HashMap::new(),
            retired: Vec::new(),
            outstanding: Arc::new(AtomicUsize::new(0)),
            stats: SurfaceStats::default(),
        }
    }

    pub fn with_waker(mut self, waker: impl Fn() + Send + Sync + 'static) -> Self {
        self.waker = Some(Arc::new(waker));
        self
    }

    pub fn registrar(&self) -> SurfaceRegistrar {
        SurfaceRegistrar(self.registrar.clone())
    }

    pub fn stats(&self) -> SurfaceStats {
        SurfaceStats {
            surfaces: self.entries.len(),
            ..self.stats
        }
    }

    pub fn latch(&mut self, target: Instant) -> bool {
        self.apply_ops();
        let mut changed = false;
        for entry in self.entries.values_mut() {
            changed |= entry.latch(target, &mut self.retired, &mut self.stats);
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
        let releasing =
            (self.outstanding.load(Ordering::Acquire) > 0).then(|| Instant::now() + RELEASE_POLL);
        let timed = self.entries.values().flat_map(|entry| {
            entry
                .pending
                .iter()
                .filter(|pending| pending.ready)
                .filter_map(|pending| pending.frame.present_at)
        });
        releasing.into_iter().chain(timed).min()
    }

    pub fn presented(&mut self, at: Instant, interval: Duration) {
        for entry in self.entries.values_mut() {
            for frame in take(&mut entry.unreported) {
                entry.emit(SurfaceEvent::Presented { frame, at });
            }
            if entry.wants_frame && entry.visible() {
                entry.wants_frame = false;
                entry.emit(SurfaceEvent::FrameRequest {
                    target: at + interval,
                });
            }
        }
    }

    pub fn maintain(&mut self) {
        let retired = take(&mut self.retired);
        if !retired.is_empty() {
            self.stats.buffers_released += retired.len() as u64;
            #[cfg(not(target_arch = "wasm32"))]
            self.release_after_gpu(retired);
            #[cfg(target_arch = "wasm32")]
            release(retired);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.outstanding.load(Ordering::Acquire) > 0 {
            let _ = self.device.poll(PollType::Poll);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn release_after_gpu(&self, retired: Vec<Retired>) {
        self.outstanding.fetch_add(1, Ordering::AcqRel);
        let outstanding = self.outstanding.clone();
        self.queue.on_submitted_work_done(move || {
            for Retired { events, buffer } in retired {
                let _ = events.send(SurfaceEvent::Released(buffer));
            }
            outstanding.fetch_sub(1, Ordering::AcqRel);
        });
    }

    pub(crate) fn latched(&self, id: SurfaceId) -> Option<&Latched> {
        self.entries.get(&id)?.current.as_ref()
    }

    pub(crate) fn holds(&self, id: SurfaceId, set: BufferSetId) -> bool {
        self.entries.get(&id).is_some_and(|entry| {
            entry.sets.contains_key(&set)
                || entry
                    .current
                    .as_ref()
                    .is_some_and(|current| current.buffer.set == set)
        })
    }

    fn apply_ops(&mut self) {
        while let Ok(op) = self.ops.try_recv() {
            match op {
                Op::Attach(source, status) => self.attach(source, status),
                Op::Place(id, placement) => {
                    if let Some(entry) = self.entries.get_mut(&id)
                        && entry.placement != Some(placement)
                        && !entry.detached
                    {
                        entry.placement = Some(placement);
                        entry.seq += 1;
                        entry.emit(SurfaceEvent::Configure(SurfaceConfig {
                            seq: entry.seq,
                            width: placement.width,
                            height: placement.height,
                            scale: placement.scale,
                            visible: placement.visible,
                        }));
                    }
                }
                Op::Detach(id) => {
                    if let Some(entry) = self.entries.get_mut(&id)
                        && !entry.detached
                    {
                        entry.detached = true;
                        entry.wants_frame = false;
                        for pending in take(&mut entry.pending) {
                            entry.discard(pending, &mut self.stats);
                        }
                        entry.emit(SurfaceEvent::Detached);
                    }
                }
                Op::Bind(id) => *self.binds.entry(id).or_default() += 1,
                Op::Unbind(id) => {
                    if let Some(count) = self.binds.get_mut(&id) {
                        *count = count.saturating_sub(1);
                    }
                }
            }
        }
    }

    fn attach(&mut self, source: SurfaceSource, status: StatusSink) {
        let SurfaceSource {
            id,
            options,
            messages,
            events,
        } = source;
        let cx = SurfaceCx {
            device: self.device.clone(),
            queue: self.queue.clone(),
            adapter: self.adapter.clone(),
            waker: self.waker.clone(),
        };
        let _ = events.send(SurfaceEvent::Attached(Box::new(cx)));
        status(SurfaceStatus::Waiting);
        self.entries.insert(
            id,
            Entry {
                options,
                messages,
                events,
                status,
                sets: HashMap::new(),
                pending: VecDeque::new(),
                current: None,
                unreported: Vec::new(),
                placement: None,
                seq: 0,
                wants_frame: false,
                detached: false,
                failed: false,
            },
        );
    }

    fn collect(&mut self) {
        let binds = &self.binds;
        let gone: Vec<_> = self
            .entries
            .iter()
            .filter(|(id, entry)| entry.detached && binds.get(id).is_none_or(|count| *count == 0))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some(entry) = self.entries.remove(&id)
                && let Some(current) = entry.current
            {
                self.retired.push(Retired {
                    events: entry.events,
                    buffer: current.buffer,
                });
            }
        }
        self.binds.retain(|_, count| *count > 0);
    }
}

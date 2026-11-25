#[cfg(feature = "profile")]
use rustc_hash::FxHashMap;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
#[cfg(feature = "profile")]
use std::cell::RefCell;
use std::ops::{AddAssign, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeapUse {
    pub allocations: u64,
    pub bytes: u64,
}

impl Sub for HeapUse {
    type Output = HeapUse;

    fn sub(self, earlier: HeapUse) -> HeapUse {
        HeapUse {
            allocations: self.allocations - earlier.allocations,
            bytes: self.bytes - earlier.bytes,
        }
    }
}

impl AddAssign for HeapUse {
    fn add_assign(&mut self, other: HeapUse) {
        self.allocations += other.allocations;
        self.bytes += other.bytes;
    }
}

thread_local! {
    static TOTAL: Cell<HeapUse> = const { Cell::new(HeapUse { allocations: 0, bytes: 0 }) };
}

struct Counting;

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

pub fn total() -> HeapUse {
    TOTAL.get()
}

fn count(size: usize) {
    #[cfg(feature = "profile")]
    if recorded(size) {
        return;
    }
    let _ = TOTAL.try_with(|total| {
        let mut used = total.get();
        used += HeapUse {
            allocations: 1,
            bytes: size as u64,
        };
        total.set(used);
    });
}

#[cfg(feature = "profile")]
const DEPTH: usize = 96;

#[cfg(feature = "profile")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Recording {
    Off,
    Armed,
    Busy,
}

#[cfg(feature = "profile")]
pub type Sites = FxHashMap<Box<[usize]>, HeapUse>;

#[cfg(feature = "profile")]
thread_local! {
    static RECORDING: Cell<Recording> = const { Cell::new(Recording::Off) };
    static SITES: RefCell<Sites> = RefCell::new(Sites::default());
}

#[cfg(feature = "profile")]
pub fn arm(on: bool) {
    RECORDING.set(if on { Recording::Armed } else { Recording::Off });
}

#[cfg(feature = "profile")]
pub fn take_sites() -> Sites {
    SITES.take()
}

#[cfg(feature = "profile")]
fn recorded(size: usize) -> bool {
    let recording = RECORDING.try_with(Cell::get).unwrap_or(Recording::Off);
    if recording != Recording::Armed {
        return recording == Recording::Busy;
    }
    RECORDING.set(Recording::Busy);
    let mut stack = [0; DEPTH];
    let mut depth = 0;
    backtrace::trace(|frame| {
        stack[depth] = frame.ip().addr();
        depth += 1;
        depth < DEPTH
    });
    let _ = SITES.try_with(|sites| {
        if let Ok(mut sites) = sites.try_borrow_mut() {
            let used = HeapUse {
                allocations: 1,
                bytes: size as u64,
            };
            if let Some(site) = sites.get_mut(&stack[..depth]) {
                *site += used;
            } else {
                sites.insert(stack[..depth].into(), used);
            }
        }
    });
    RECORDING.set(Recording::Armed);
    false
}

#[cfg(feature = "dhat")]
use dhat::Alloc as Inner;
#[cfg(not(feature = "dhat"))]
use std::alloc::System as Inner;
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;

pub(crate) fn total() -> (u64, u64) {
    HEAP.get()
}

thread_local! {
    static HEAP: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

struct Counting;

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { Inner.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { Inner.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { Inner.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { Inner.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count(size: usize) {
    let _ = HEAP.try_with(|heap| {
        let (allocations, bytes) = heap.get();
        heap.set((allocations + 1, bytes + size as u64));
    });
}

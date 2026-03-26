use bladeink::story::Story;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, Ordering};

mod common;

static ALLOCATED: AtomicI64 = AtomicI64::new(0);

struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        ALLOCATED.fetch_sub(layout.size() as i64, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            ALLOCATED.fetch_sub(layout.size() as i64, Ordering::Relaxed);
            ALLOCATED.fetch_add(new_size as i64, Ordering::Relaxed);
        }
        new_ptr
    }
}

#[global_allocator]
static GLOBAL: TrackingAllocator = TrackingAllocator;

fn run_story_to_end(json_string: &str) {
    let mut story = Story::new(json_string).expect("failed to create story");
    while story.can_continue() || !story.get_current_choices().is_empty() {
        while story.can_continue() {
            story.cont().expect("cont failed");
        }
        if !story.get_current_choices().is_empty() {
            story.choose_choice_index(0).expect("choose failed");
        }
    }
}

#[test]
fn the_intercept_on_loop_test() {
    const THREADS: usize = 10;
    const STORIES_PER_THREAD: usize = 100;

    let json_string = common::get_json_string("inkfiles/TheIntercept.ink.json").unwrap();

    let before = ALLOCATED.load(Ordering::SeqCst);

    std::thread::scope(|s| {
        let json = &json_string;
        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                s.spawn(move || {
                    for _ in 0..STORIES_PER_THREAD {
                        run_story_to_end(json);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread panicked");
        }
    });

    let after = ALLOCATED.load(Ordering::SeqCst);
    let leaked = after - before;

    assert_eq!(
        leaked,
        0,
        "Memory leak detected: {} bytes still allocated after dropping {} stories",
        leaked,
        THREADS * STORIES_PER_THREAD
    );
}

//! Compare owned heap and construction time for the legacy and flat JSON paths.
//! Run with: cargo run --release -p bladeink --example flat_story_cost

use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use bladeink::{flat_story_player::FlatStory, story::LegacyStory};

struct CountingAllocator;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn record_growth(bytes: usize) {
    let current = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(current, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_growth(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_growth(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, new_size) };
        if !result.is_null() {
            if new_size >= layout.size() {
                record_growth(new_size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn measure<T>(label: &str, build: impl FnOnce() -> T, run: impl FnOnce(&mut T)) {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let started = Instant::now();
    let mut story = black_box(build());
    let create_time = started.elapsed();
    let owned = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
    let create_peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    let started = Instant::now();
    run(&mut story);
    let run_time = started.elapsed();
    let run_peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    println!(
        "{label}: create={create_time:?} owned={owned} B create_peak={create_peak} B run={run_time:?} run_peak={run_peak} B"
    );
    drop(story);
}

fn main() {
    let json = include_str!("../../conformance-tests/inkfiles/TheIntercept.ink.json");
    for _ in 0..5 {
        measure(
            "legacy",
            || LegacyStory::new_with_seed(json, 1).unwrap(),
            |story| {
                for _ in 0..20 {
                    while story.can_continue() {
                        black_box(story.cont().unwrap());
                    }
                    if story.get_current_choices().is_empty() {
                        break;
                    }
                    story.choose_choice_index(0).unwrap();
                }
            },
        );
        measure(
            "flat",
            || FlatStory::new_with_seed(json, 1).unwrap(),
            |story| {
                for _ in 0..20 {
                    while story.can_continue() {
                        black_box(story.cont().unwrap());
                    }
                    if story.get_current_choices().is_empty() {
                        break;
                    }
                    story.choose_choice_index(0).unwrap();
                }
            },
        );
    }
}

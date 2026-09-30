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

#[cfg(feature = "load-profile")]
#[derive(Clone, Copy)]
struct PhaseTimes {
    json_decode: Option<std::time::Duration>,
    arena_build: Option<std::time::Duration>,
    stream_parse_and_build: Option<std::time::Duration>,
    targets: std::time::Duration,
    runtime: std::time::Duration,
}

fn measure<T, D>(
    label: &str,
    build: impl FnOnce() -> T,
    run: impl FnOnce(&mut T),
    detail: impl FnOnce(&T) -> D,
) -> (std::time::Duration, D) {
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
    let detail = detail(&story);
    drop(story);
    (create_time, detail)
}

fn run_legacy(story: &mut LegacyStory) {
    for _ in 0..20 {
        while story.can_continue() {
            black_box(story.cont().unwrap());
        }
        if story.get_current_choices().is_empty() {
            break;
        }
        story.choose_choice_index(0).unwrap();
    }
}

fn run_flat(story: &mut FlatStory) {
    for _ in 0..20 {
        while story.can_continue() {
            black_box(story.cont().unwrap());
        }
        if story.get_current_choices().is_empty() {
            break;
        }
        story.choose_choice_index(0).unwrap();
    }
}

fn measure_legacy(json: &str) -> std::time::Duration {
    measure(
        "legacy",
        || LegacyStory::new_with_seed(json, 1).unwrap(),
        run_legacy,
        |_| {},
    )
    .0
}

#[cfg(feature = "load-profile")]
fn measure_flat(json: &str) -> (std::time::Duration, Option<PhaseTimes>) {
    measure(
        "flat",
        || FlatStory::new_with_seed_profiled(json, 1).unwrap(),
        |(story, _)| run_flat(story),
        |(_, profile)| {
            println!(
                "  phases: json_decode={:?} arena_build={:?} stream_parse_and_build={:?} targets={:?} runtime={:?}",
                profile.json_decode,
                profile.arena_build,
                profile.stream_parse_and_build,
                profile.targets,
                profile.runtime,
            );
            Some(PhaseTimes {
                json_decode: profile.json_decode,
                arena_build: profile.arena_build,
                stream_parse_and_build: profile.stream_parse_and_build,
                targets: profile.targets,
                runtime: profile.runtime,
            })
        },
    )
}

#[cfg(not(feature = "load-profile"))]
fn measure_flat(json: &str) -> (std::time::Duration, Option<()>) {
    measure(
        "flat",
        || FlatStory::new_with_seed(json, 1).unwrap(),
        run_flat,
        |_| None,
    )
}

fn median(samples: &mut [std::time::Duration]) -> std::time::Duration {
    samples.sort_unstable();
    (samples[(samples.len() - 1) / 2] + samples[samples.len() / 2]) / 2
}

#[cfg(feature = "load-profile")]
fn phase_median(
    samples: &[PhaseTimes],
    get: impl Fn(&PhaseTimes) -> Option<std::time::Duration>,
) -> Option<std::time::Duration> {
    let mut values: Vec<_> = samples.iter().filter_map(get).collect();
    (!values.is_empty()).then(|| median(&mut values))
}

fn main() {
    let json = include_str!("../../conformance-tests/inkfiles/TheIntercept.ink.json");
    let mut legacy = Vec::new();
    let mut flat = Vec::new();
    let mut phases = Vec::new();
    for iteration in 0..12 {
        let (legacy_time, (flat_time, phase)) = if iteration % 2 == 0 {
            (measure_legacy(json), measure_flat(json))
        } else {
            let flat_sample = measure_flat(json);
            (measure_legacy(json), flat_sample)
        };
        if iteration >= 2 {
            legacy.push(legacy_time);
            flat.push(flat_time);
            if let Some(phase) = phase {
                phases.push(phase);
            }
        }
    }
    println!(
        "median create after two warmup pairs: legacy={:?} flat={:?}",
        median(&mut legacy),
        median(&mut flat)
    );
    #[cfg(feature = "load-profile")]
    println!(
        "median phases: json_decode={:?} arena_build={:?} stream_parse_and_build={:?} targets={:?} runtime={:?}",
        phase_median(&phases, |p| p.json_decode),
        phase_median(&phases, |p| p.arena_build),
        phase_median(&phases, |p| p.stream_parse_and_build),
        phase_median(&phases, |p| Some(p.targets)),
        phase_median(&phases, |p| Some(p.runtime)),
    );
    #[cfg(not(feature = "load-profile"))]
    let _ = phases;
}

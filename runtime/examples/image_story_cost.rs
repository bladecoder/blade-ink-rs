//! Measure host heap usage when opening and playing JSON and binary story images.
//! Run with: cargo run --release -p bladeink --example image_story_cost --features binary-image -- story.json story.inkb

use std::{
    alloc::{GlobalAlloc, Layout, System},
    env, fs,
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use bladeink::{image::compile_json_to_image, story::Story};

struct CountingAllocator;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

fn grew(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            grew(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            grew(layout.size());
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
            CALLS.fetch_add(1, Ordering::Relaxed);
            if new_size >= layout.size() {
                grew(new_size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Clone, Copy, Default)]
struct Sample {
    open_time: Duration,
    open_retained: usize,
    open_peak: usize,
    open_calls: usize,
    play_time: Duration,
    play_retained: usize,
    play_peak: usize,
    play_calls: usize,
    lines: usize,
    choices: usize,
}

fn play(story: &mut Story) -> (usize, usize) {
    let mut lines = 0;
    let mut choices = 0;
    while lines < 2_000 && choices < 20 {
        while story.can_continue() && lines < 2_000 {
            black_box(story.cont().unwrap());
            lines += 1;
        }
        let available = story.get_current_choices();
        if available.is_empty() {
            break;
        }
        story.choose_choice_index(0).unwrap();
        choices += 1;
    }
    (lines, choices)
}

fn verify_same_first_path(json: &str, image: &'static [u8]) {
    let mut from_json = Story::new_with_seed(json, 1).unwrap();
    let mut from_image = Story::new_from_image_with_seed(image, 1).unwrap();
    let mut lines = 0;
    for _ in 0..20 {
        while from_json.can_continue() && lines < 2_000 {
            assert!(from_image.can_continue());
            assert_eq!(from_json.cont().unwrap(), from_image.cont().unwrap());
            assert_eq!(
                from_json.get_current_tags().unwrap(),
                from_image.get_current_tags().unwrap()
            );
            lines += 1;
        }
        assert_eq!(from_json.can_continue(), from_image.can_continue());
        let json_choices = from_json.get_current_choices();
        let image_choices = from_image.get_current_choices();
        assert_eq!(json_choices.len(), image_choices.len());
        for (json_choice, image_choice) in json_choices.iter().zip(image_choices.iter()) {
            assert_eq!(json_choice.text, image_choice.text);
            assert_eq!(json_choice.tags, image_choice.tags);
        }
        if json_choices.is_empty() || lines == 2_000 {
            break;
        }
        from_json.choose_choice_index(0).unwrap();
        from_image.choose_choice_index(0).unwrap();
    }
}

fn measure(build: impl FnOnce() -> Story, execute: bool) -> Sample {
    let baseline = LIVE.load(Ordering::Relaxed);
    let calls = CALLS.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let start = Instant::now();
    let mut story = black_box(build());
    let open_time = start.elapsed();
    let open_retained = LIVE.load(Ordering::Relaxed) - baseline;
    let open_peak = PEAK.load(Ordering::Relaxed) - baseline;
    let open_calls = CALLS.load(Ordering::Relaxed) - calls;

    let mut result = Sample {
        open_time,
        open_retained,
        open_peak,
        open_calls,
        ..Sample::default()
    };
    if execute {
        let calls = CALLS.load(Ordering::Relaxed);
        PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
        let start = Instant::now();
        (result.lines, result.choices) = play(&mut story);
        result.play_time = start.elapsed();
        result.play_retained = LIVE.load(Ordering::Relaxed) - baseline;
        result.play_peak = PEAK.load(Ordering::Relaxed) - baseline;
        result.play_calls = CALLS.load(Ordering::Relaxed) - calls;
    }
    drop(story);
    assert_eq!(LIVE.load(Ordering::Relaxed), baseline);
    result
}

fn median<T: Ord + Copy>(values: impl Iterator<Item = T>) -> T {
    let mut values: Vec<T> = values.collect();
    values.sort_unstable();
    values[values.len() / 2]
}

fn report(label: &str, samples: &[Sample]) {
    println!(
        "{label}: open_time_us={} open_retained_B={} open_peak_B={} open_calls={} play_time_us={} play_retained_B={} play_peak_B={} play_calls={} lines={} choices={}",
        median(samples.iter().map(|sample| sample.open_time.as_micros())),
        median(samples.iter().map(|sample| sample.open_retained)),
        median(samples.iter().map(|sample| sample.open_peak)),
        median(samples.iter().map(|sample| sample.open_calls)),
        median(samples.iter().map(|sample| sample.play_time.as_micros())),
        median(samples.iter().map(|sample| sample.play_retained)),
        median(samples.iter().map(|sample| sample.play_peak)),
        median(samples.iter().map(|sample| sample.play_calls)),
        median(samples.iter().map(|sample| sample.lines)),
        median(samples.iter().map(|sample| sample.choices)),
    );
}

fn synthetic_json(node_count: usize) -> String {
    format!(
        "{{\"inkVersion\":21,\"root\":[{}\"done\",null],\"listDefs\":{{}}}}",
        "\"^x\",".repeat(node_count)
    )
}

fn scale_check() {
    let small = compile_json_to_image(synthetic_json(1).as_bytes()).unwrap();
    let large = compile_json_to_image(synthetic_json(2_000).as_bytes()).unwrap();
    let small_len = small.len();
    let large_len = large.len();
    let small = Box::leak(small.into_boxed_slice());
    let large = Box::leak(large.into_boxed_slice());
    let small_sample = measure(|| Story::new_from_image_with_seed(small, 1).unwrap(), false);
    let large_sample = measure(|| Story::new_from_image_with_seed(large, 1).unwrap(), false);
    println!(
        "scale: small_nodes=1 small_image_B={small_len} small_retained_B={} large_nodes=2000 large_image_B={large_len} large_retained_B={}",
        small_sample.open_retained, large_sample.open_retained
    );
    assert_eq!(small_sample.open_retained, large_sample.open_retained);
}

fn main() {
    let mut args = env::args().skip(1);
    let json_path = args.next().expect("expected a compiled JSON path");
    let image_path = args.next().expect("expected a binary image path");
    assert!(args.next().is_none(), "expected only two paths");
    let json = fs::read_to_string(&json_path).unwrap();
    let image = fs::read(&image_path).unwrap();
    println!("source_json_B={} image_B={}", json.len(), image.len());
    // The image is leaked only to model a static flash slice on a host. Its
    // allocation and the input JSON are outside all measured intervals.
    let image = Box::leak(image.into_boxed_slice());
    verify_same_first_path(&json, image);
    let mut json_samples = Vec::new();
    let mut image_samples = Vec::new();
    for iteration in 0..7 {
        let json_sample = || measure(|| Story::new_with_seed(&json, 1).unwrap(), true);
        let image_sample = || measure(|| Story::new_from_image_with_seed(image, 1).unwrap(), true);
        let (json_result, image_result) = if iteration % 2 == 0 {
            (json_sample(), image_sample())
        } else {
            let image_result = image_sample();
            (json_sample(), image_result)
        };
        assert_eq!(
            (json_result.lines, json_result.choices),
            (image_result.lines, image_result.choices)
        );
        if iteration > 0 {
            json_samples.push(json_result);
            image_samples.push(image_result);
        }
    }
    report("json", &json_samples);
    report("image", &image_samples);
    scale_check();
}

//! `rinklecate` — compile and play Ink stories from the command line.
//!
//! Mirrors the interface of the official `inklecate` tool and the
//! blade-ink-java `CommandLineTool`.
//!
//! Usage: rinklecate <options> <ink, ink.json or inkb file>
//!    -o <filename>   Output file name
//!    --image         Write a binary story image (.inkb)
//!    -c              Count all visits to knots, stitches and weave points
//!    -p              Play mode
//!    -j              JSON output mode (for communication with tools like Inky)
//!    -s              Print stats about story (word count, knots, etc.)
//!    -v              Verbose mode — print compilation timings
//!    -k              Keep rinklecate running in play mode after story is complete
//!    -x <directory>  Import plugins (accepted but ignored — not supported in this implementation)

mod compiler_tool;
mod player;

use std::process;
use std::time::Instant;

pub const EXIT_CODE_ERROR: i32 = 1;

#[derive(Debug)]
pub struct Options {
    pub verbose: bool,
    pub play_mode: bool,
    pub stats: bool,
    pub json_output: bool,
    pub image_output: bool,
    pub input_file: Option<String>,
    pub output_file: Option<String>,
    pub count_all_visits: bool,
    pub keep_open_after_story_finish: bool,
    /// Plugin directories — accepted for interface compatibility but ignored.
    pub plugin_directories: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            verbose: false,
            play_mode: false,
            stats: false,
            json_output: false,
            image_output: false,
            input_file: None,
            output_file: None,
            // Match inklecate: always count visits by default.
            count_all_visits: true,
            keep_open_after_story_finish: false,
            plugin_directories: Vec::new(),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let opts = match parse_arguments(&args) {
        Some(o) => o,
        None => {
            print_usage();
            process::exit(EXIT_CODE_ERROR);
        }
    };

    if opts.input_file.is_none() {
        print_usage();
        process::exit(EXIT_CODE_ERROR);
    }

    if let Err(e) = run(opts) {
        eprintln!("{e}");
        process::exit(EXIT_CODE_ERROR);
    }
}

fn run(mut opts: Options) -> anyhow::Result<()> {
    use std::path::Path;

    let input_file = opts.input_file.as_ref().unwrap().clone();
    if opts.image_output && (opts.play_mode || opts.stats) {
        anyhow::bail!("--image cannot be combined with -p or -s");
    }

    // Resolve input path to absolute
    let working_dir = std::env::current_dir()?;
    let full_input = {
        let p = Path::new(&input_file);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            working_dir.join(p)
        }
    };

    if !full_input.exists() {
        anyhow::bail!("Could not open file '{}'", input_file);
    }

    let input_base_dir = full_input.parent().map(|p| p.to_path_buf());
    let filename_only = full_input
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let extension = full_input
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if !opts.plugin_directories.is_empty() {
        eprintln!(
            "Warning: -x (plugin directories) is not supported in this implementation and will be ignored."
        );
    }

    if extension.eq_ignore_ascii_case("inkb") {
        if opts.stats {
            anyhow::bail!("Cannot show stats for .inkb, only for .ink");
        }
        if opts.image_output {
            anyhow::bail!("--image requires .ink or .json input");
        }
        let t0 = Instant::now();
        // Story images currently borrow static bytes. The CLI loads one image
        // for the lifetime of its process, so retain this buffer until exit.
        let bytes = Box::leak(std::fs::read(&full_input)?.into_boxed_slice());
        let story = bladeink::story::Story::new_from_image_validated(bytes)
            .map_err(|e| anyhow::anyhow!("Failed to load story: {e}"))?;
        if opts.verbose {
            eprintln!(
                "Story loaded in {:.1}ms",
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
        opts.play_mode = true;
        return player::play(story, &opts);
    }

    // Resolve output path
    if opts.output_file.is_none() {
        let output_name = if opts.image_output {
            let base = filename_only
                .strip_suffix(".ink.json")
                .unwrap_or(&filename_only);
            change_extension(base, ".inkb")
        } else {
            change_extension(&filename_only, ".ink.json")
        };
        let out = input_base_dir
            .as_deref()
            .unwrap_or(&working_dir)
            .join(output_name);
        opts.output_file = Some(out.to_string_lossy().to_string());
    } else {
        // If output was given as relative, resolve it relative to input dir
        let out_path = Path::new(opts.output_file.as_ref().unwrap());
        if !out_path.is_absolute() {
            let resolved = input_base_dir
                .as_deref()
                .unwrap_or(&working_dir)
                .join(out_path);
            opts.output_file = Some(resolved.to_string_lossy().to_string());
        }
    }

    let input_string = std::fs::read_to_string(&full_input)?;
    // Strip UTF-8 BOM if present
    let input_string = input_string
        .strip_prefix('\u{feff}')
        .unwrap_or(&input_string)
        .to_owned();

    let input_is_json = extension.eq_ignore_ascii_case("json");

    if input_is_json && opts.stats {
        anyhow::bail!("Cannot show stats for .json, only for .ink");
    }

    if input_is_json {
        if opts.image_output {
            let t0 = Instant::now();
            let image = bladeink::image::compile_json_to_image(input_string.as_bytes())?;
            let output_path = opts.output_file.as_ref().unwrap();
            std::fs::write(output_path, image).map_err(|error| {
                anyhow::anyhow!(
                    "Could not write to output file '{}': {}",
                    output_path,
                    error
                )
            })?;
            if opts.verbose {
                eprintln!(
                    "Image compiled in {:.1}ms",
                    t0.elapsed().as_secs_f64() * 1000.0
                );
            }
        } else {
            // Play directly from compiled JSON — force play mode
            opts.play_mode = true;
            let t0 = Instant::now();
            let story = bladeink::story::Story::new(&input_string)
                .map_err(|e| anyhow::anyhow!("Failed to load story: {e}"))?;
            if opts.verbose {
                eprintln!(
                    "Story loaded in {:.1}ms",
                    t0.elapsed().as_secs_f64() * 1000.0
                );
            }
            player::play(story, &opts)?;
        }
    } else {
        // Compile .ink
        let t0 = Instant::now();
        let result = compiler_tool::compile(&input_string, &filename_only, input_base_dir, &opts);
        if opts.verbose {
            eprintln!(
                "Compilation took {:.1}ms",
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
        result?;
    }

    Ok(())
}

fn parse_arguments(args: &[String]) -> Option<Options> {
    if args.is_empty() {
        return None;
    }

    let mut opts = Options::default();
    let mut i = 0;
    let mut next_is_output = false;
    let mut next_is_plugin_dir = false;

    while i < args.len() {
        let arg = &args[i];

        if next_is_output {
            opts.output_file = Some(arg.clone());
            next_is_output = false;
            i += 1;
            continue;
        }

        if next_is_plugin_dir {
            opts.plugin_directories.push(arg.clone());
            next_is_plugin_dir = false;
            i += 1;
            continue;
        }

        if arg.starts_with('-') && arg.len() > 1 {
            if arg == "--image" {
                opts.image_output = true;
                i += 1;
                continue;
            }
            for ch in arg.chars().skip(1) {
                match ch {
                    'p' => opts.play_mode = true,
                    'j' => opts.json_output = true,
                    'v' => opts.verbose = true,
                    's' => opts.stats = true,
                    'c' => opts.count_all_visits = true,
                    'k' => opts.keep_open_after_story_finish = true,
                    'o' => next_is_output = true,
                    'x' => next_is_plugin_dir = true,
                    other => eprintln!("Warning: unsupported argument '-{other}' ignored"),
                }
            }
        } else {
            // Any non-flag argument is the input file (last one wins, matching Java behaviour)
            opts.input_file = Some(arg.clone());
        }

        i += 1;
    }

    Some(opts)
}

fn print_usage() {
    eprintln!(
        "Usage: rinklecate <options> <ink, ink.json or inkb file>
   -o <filename>   Output file name
   --image         Write a binary story image (.inkb)
   -c              Count all visits to knots, stitches and weave points, not
                   just those referenced by TURNS_SINCE and read counts.
   -p              Play mode
   -j              Output in JSON format (for communication with tools like Inky)
   -s              Print stats about story including word count
   -v              Verbose mode - print compilation timings
   -k              Keep rinklecate running in play mode even after story is complete
   -x <directory>  Import plugins for the compiler (not supported, ignored)"
    );
}

pub fn change_extension(filename: &str, extension: &str) -> String {
    match filename.rfind('.') {
        Some(pos) => format!("{}{}", &filename[..pos], extension),
        None => format!("{}{}", filename, extension),
    }
}

//! `rinklecate` — compile and play Ink stories from the command line.
//!
//! Mirrors the interface of the official `inklecate` tool and the
//! blade-ink-java `CommandLineTool`.
//!
//! Usage: rinklecate <options> <ink file>
//!    -o <filename>   Output file name
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

#[derive(Debug, Default)]
pub struct Options {
    pub verbose: bool,
    pub play_mode: bool,
    pub stats: bool,
    pub json_output: bool,
    pub input_file: Option<String>,
    pub output_file: Option<String>,
    pub count_all_visits: bool,
    pub keep_open_after_story_finish: bool,
    /// Plugin directories — accepted for interface compatibility but ignored.
    pub plugin_directories: Vec<String>,
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

    // Resolve output path
    if opts.output_file.is_none() {
        let out = input_base_dir
            .as_deref()
            .unwrap_or(&working_dir)
            .join(change_extension(&filename_only, ".ink.json"));
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

    let input_is_json = filename_only.to_lowercase().ends_with(".json");

    if input_is_json && opts.stats {
        anyhow::bail!("Cannot show stats for .json, only for .ink");
    }

    if !opts.plugin_directories.is_empty() {
        eprintln!(
            "Warning: -x (plugin directories) is not supported in this implementation and will be ignored."
        );
    }

    if input_is_json {
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
        "Usage: rinklecate <options> <ink file>
   -o <filename>   Output file name
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

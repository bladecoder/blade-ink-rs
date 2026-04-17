//! Compilation logic: `.ink` → `.ink.json`.

use std::cell::RefCell;
use std::path::PathBuf;

use bladeink_compiler::{Compiler, CompilerError, CompilerOptions};

use crate::Options;

/// Compile the given ink source.
///
/// - If `opts.stats` is set: print stats and return (no JSON written, no play).
/// - Otherwise: compile to JSON, write to `opts.output_file`, then optionally play.
pub fn compile(
    source: &str,
    filename: &str,
    base_dir: Option<PathBuf>,
    opts: &Options,
) -> anyhow::Result<()> {
    let errors: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let warnings: RefCell<Vec<String>> = RefCell::new(Vec::new());

    let compiler_opts = CompilerOptions {
        count_all_visits: opts.count_all_visits,
        source_filename: Some(filename.to_owned()),
    };
    let compiler = Compiler::with_options(compiler_opts);

    // ------------------------------------------------------------------
    // Stats mode: parse only, print stats, return early.
    // ------------------------------------------------------------------
    if opts.stats {
        let stats_result = if let Some(ref dir) = base_dir {
            let dir = dir.clone();
            compiler.compile_to_stats_with_file_handler(source, move |inc| {
                let path = dir.join(inc);
                std::fs::read_to_string(&path).map_err(|e| {
                    CompilerError::invalid_source(format!(
                        "Failed to read included file '{}': {}",
                        inc, e
                    ))
                })
            })
        } else {
            compiler.compile_to_stats(source)
        };

        match stats_result {
            Ok(stats) => {
                if opts.json_output {
                    println!(
                        "{{\"stats\":{{\"words\":{},\"knots\":{},\"stitches\":{},\"functions\":{},\"choices\":{},\"gathers\":{},\"diverts\":{}}}}}",
                        stats.words,
                        stats.knots,
                        stats.stitches,
                        stats.functions,
                        stats.choices,
                        stats.gathers,
                        stats.diverts,
                    );
                } else {
                    println!("Words: {}", stats.words);
                    println!("Knots: {}", stats.knots);
                    println!("Stitches: {}", stats.stitches);
                    println!("Functions: {}", stats.functions);
                    println!("Choices: {}", stats.choices);
                    println!("Gathers: {}", stats.gathers);
                    println!("Diverts: {}", stats.diverts);
                }
            }
            Err(e) => {
                errors.borrow_mut().push(e.to_string());
                print_all_messages(&errors.borrow(), &warnings.borrow(), opts.json_output);
                anyhow::bail!("Compilation failed");
            }
        }
        return Ok(());
    }

    // ------------------------------------------------------------------
    // Normal compilation: produce JSON.
    // ------------------------------------------------------------------
    let json_result = if let Some(ref dir) = base_dir {
        let dir = dir.clone();
        compiler.compile_with_file_handler(source, move |inc| {
            let path = dir.join(inc);
            std::fs::read_to_string(&path).map_err(|e| {
                CompilerError::invalid_source(format!(
                    "Failed to read included file '{}': {}",
                    inc, e
                ))
            })
        })
    } else {
        compiler.compile(source)
    };

    let json_string = match json_result {
        Ok(s) => s,
        Err(e) => {
            errors.borrow_mut().push(e.to_string());
            if opts.json_output {
                println!("{{\"compile-success\": false}}");
            }
            print_all_messages(&errors.borrow(), &warnings.borrow(), opts.json_output);
            anyhow::bail!("Compilation failed");
        }
    };

    let compile_success = errors.borrow().is_empty();

    if opts.json_output {
        if compile_success {
            println!("{{\"compile-success\": true}}");
        } else {
            println!("{{\"compile-success\": false}}");
        }
    }

    print_all_messages(&errors.borrow(), &warnings.borrow(), opts.json_output);

    if !compile_success {
        anyhow::bail!("Compilation failed");
    }

    // ------------------------------------------------------------------
    // Write output JSON file (unless in play-only mode with no output needed).
    // ------------------------------------------------------------------
    if !opts.play_mode {
        let output_path = opts.output_file.as_ref().unwrap();
        std::fs::write(output_path, &json_string).map_err(|e| {
            anyhow::anyhow!("Could not write to output file '{}': {}", output_path, e)
        })?;
        if opts.json_output {
            println!("{{\"export-complete\": true}}");
        }
    }

    // ------------------------------------------------------------------
    // Play mode: run the story interactively.
    // ------------------------------------------------------------------
    if opts.play_mode {
        crate::player::play_from_json(&json_string, opts)?;
    }

    Ok(())
}

fn print_all_messages(errors: &[String], warnings: &[String], json_output: bool) {
    if json_output {
        if !errors.is_empty() || !warnings.is_empty() {
            let all: Vec<String> = warnings
                .iter()
                .chain(errors.iter())
                .map(|s| serde_json::to_string(s).expect("serializing issue message to JSON"))
                .collect();
            println!("{{\"issues\": [{}]}}", all.join(", "));
        }
    } else {
        for msg in warnings {
            eprintln!("{msg}");
        }
        for msg in errors {
            eprintln!("{msg}");
        }
    }
}

//! Console player that can runs compiled `.ink.json` story files writen in the
//! **Ink** language.
use std::cell::RefCell;

use std::{error::Error, fs, io, io::Write, path::Path, rc::Rc};

use anyhow::Context;
use bladeink::{
    choice::Choice,
    story::{
        errors::{ErrorHandler, ErrorType},
        Story,
    },
};
use clap::Parser;
use rand::Rng;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The compiled .ink.json file
    pub json_filename: String,

    /// Choose options randomly
    #[arg(short, default_value_t = false)]
    pub auto_play: bool,

    /// Forbid external function fallbacks
    #[arg(short = 'e', default_value_t = false)]
    pub forbid_external_fallbacks: bool,
}

enum Command {
    Choose(usize),
    Exit(),
    Help(),
    Load(String),
    Save(String),
    DivertPath(String),
    Flow(String),
}

struct EHandler {
    pub should_terminate: bool,
}

impl EHandler {
    pub fn new() -> Rc<RefCell<EHandler>> {
        Rc::new(RefCell::new(EHandler {
            should_terminate: false,
        }))
    }
}

impl ErrorHandler for EHandler {
    fn error(&mut self, message: &str, error_type: ErrorType) {
        eprintln!("{}", message);

        if error_type == ErrorType::Error {
            self.should_terminate = true;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let json_string = get_json_string(&args.json_filename)?;

    // REMOVE BOM if exists
    let json_string_without_bom = json_string.strip_prefix('\u{feff}').unwrap_or(&json_string);

    let mut story = Story::new(json_string_without_bom)?;
    let err_handler = EHandler::new();
    story.set_error_handler(err_handler.clone());
    story.set_allow_external_function_fallbacks(!args.forbid_external_fallbacks);

    let mut end = false;

    while !end && !err_handler.borrow().should_terminate {
        while story.can_continue() {
            let line = story.cont()?;

            print!("{}", line);

            let tags = story.get_current_tags()?;

            if !tags.is_empty() {
                println!("# tags: {}", tags.join(", "));
            }
        }

        let choices = story.get_current_choices();
        if !choices.is_empty() {
            let command = if args.auto_play {
                let i = rand::thread_rng().gen_range(0..choices.len());

                println!();
                print_choices(&choices);
                println!("?> {}", i + 1);

                Command::Choose(i)
            } else {
                read_input(&choices)?
            };

            end = process_command(command, &mut story)?;
        } else {
            end = true;
        }
    }

    Ok(())
}

// Returns true if the program has to stop
fn process_command(command: Command, story: &mut Story) -> Result<bool, Box<dyn Error>> {
    match command {
        Command::Choose(c) => story.choose_choice_index(c)?,
        Command::Exit() => return Ok(true),
        Command::Load(filename) => {
            let saved_string = get_json_string(&filename)?;
            story.load_state(&saved_string)?;
            println!("Ok.")
        }
        Command::Save(filename) => {
            let json_string = story.save_state()?;
            save_json(&filename, &json_string)?;
            println!("Ok.")
        }
        Command::Flow(flow) => {
            let result = story.switch_flow(&flow);

            if let Err(desc) = result {
                println!("<error switching to '{flow}': {desc}>")
            }
        }
        Command::DivertPath(path) => {
            let result = story.choose_path_string(&path, true, None);

            if let Err(desc) = result {
                println!("<error diverting to '{path}': {desc}>")
            }
        }
        Command::Help() => println!(
            "Commands:\n\tload <filename>\n\tsave <filename>\n\t-> <divert_path>\n\tswitch <flow_name>\n\tquit\n\t"
        ),
    }

    Ok(false)
}

fn print_choices(choices: &[Rc<Choice>]) {
    for (i, c) in choices.iter().enumerate() {
        println!("{}: {}", i + 1, c.text);
    }
}

fn read_input(choices: &Vec<Rc<Choice>>) -> Result<Command, Box<dyn Error>> {
    let mut line = String::new();

    loop {
        println!();
        print_choices(choices);
        print!("?> ");
        io::stdout().flush()?;

        line.clear();
        let _b1 = std::io::stdin().read_line(&mut line)?;

        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.len() == 1 {
            match trimmed.parse::<i32>() {
                Ok(v) => {
                    if v < 1 || v > choices.len() as i32 {
                        print_error("option out of range");
                        continue;
                    } else {
                        return Ok(Command::Choose((v - 1) as usize));
                    }
                }
                Err(_) => {
                    print_error("unrecognized option or command");
                    continue;
                }
            }
        }

        let words: Vec<&str> = trimmed.split_whitespace().collect();

        match words[0].trim().to_lowercase().as_str() {
            "exit" | "quit" => return Ok(Command::Exit()),
            "help" => return Ok(Command::Help()),
            "load" => {
                if words.len() == 2 {
                    return Ok(Command::Load(words[1].trim().to_string()));
                }

                print_error("incorrect filename");
            }
            "save" => {
                if words.len() == 2 {
                    return Ok(Command::Save(words[1].trim().to_string()));
                }

                print_error("incorrect filename");
            }

            "switch" => {
                if words.len() == 2 {
                    return Ok(Command::Flow(words[1].trim().to_string()));
                }

                print_error("incorrect flow name");
            }

            "->" => {
                if words.len() == 2 {
                    return Ok(Command::DivertPath(words[1].trim().to_string()));
                }

                print_error("incorrect divert");
            }
            _ => print_error("unrecognized option or command"),
        }
    }
}

fn print_error(error: &str) {
    eprintln!("<{error}>");
}

fn get_json_string(filename: &str) -> Result<String, Box<dyn Error>> {
    let path = Path::new(filename);
    let json = fs::read_to_string(path)
        .with_context(|| format!("could not read file `{}`", path.to_string_lossy()))?;

    Ok(json)
}

fn save_json(filename: &str, content: &str) -> Result<(), Box<dyn Error>> {
    let path = Path::new(filename);
    fs::write(path, content)
        .with_context(|| format!("could not write file `{}`", path.to_string_lossy()))?;

    Ok(())
}

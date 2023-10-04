use std::{path::Path, fs, error::Error, rc::Rc, io};
use std::io::Write; 

use anyhow::Context;
use bink::{story::Story, choice::Choice};
use clap::Parser;


#[derive(Parser)]
struct Args {
    pub json_filename: String,
}

enum Command {
    Choose(usize),
    Exit(),
    Help(),
    Load(String),
    Save(String),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let json_string = get_json_string(&args.json_filename)?;

    // REMOVE BOM if exits
    let json_string_without_bom = json_string.strip_prefix("\u{feff}").unwrap_or(&json_string);

    let mut story = Story::new(json_string_without_bom)?;
    let mut end = false;
    
    while !end {
        while story.can_continue() {
            let line = story.cont()?;
            let trimmed = line.trim();

            println!("{}", trimmed);
        }

        let choices = story.get_current_choices();
        if !choices.is_empty() {
            let command = read_input(&choices)?;
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
        Command::Choose(c) => story.choose_choice_index(c),
        Command::Exit() => return Ok(true),
        Command::Load(filename) => {
            let saved_string = get_json_string(&filename)?;
            story.get_state_mut().load_json(&saved_string)?;
            println!("Ok.")
        },
        Command::Save(filename) => {
            let json_string = story.get_state().to_json()?;
            save_json(&filename, &json_string)?;
        },
        Command::Help() => println!("Commands:\n\tload <filename>\n\tsave <filename>\n\tquit\n\t"),
    }

    Ok(false)
}

fn print_choices(choices: &[Rc<Choice>]) {
    for (i, c) in choices.iter().enumerate() {
        println!("{}. {}", i + 1, c.text);
    }
}

fn read_input(choices: &Vec<Rc<Choice>>) -> Result<Command, Box<dyn Error>> {
    let mut line = String::new();

    loop {
        println!();
        print_choices(choices);
        println!();
        print!("?>");
        io::stdout().flush()?;

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
                    } else {
                        return Ok(Command::Choose((v - 1) as usize));
                    }
                },
                Err(_) => print_error("unrecognized option or command"),
            }
        }

        let words:Vec<&str> = trimmed.split_whitespace().collect();

        match words[0].trim().to_lowercase().as_str() {
            "exit" | "quit" => return Ok(Command::Exit()),
            "help" => return Ok(Command::Help()),
            "load" => {
                if words.len() == 2 {
                    return Ok(Command::Load(words[1].trim().to_string()))
                }

                print_error("incorrect filename");
            },
            "save" => {return Ok(Command::Save(words[1].trim().to_string()))},
            _ =>  print_error("unrecognized option or command"),           
        }
    }
}

fn print_error(error: &str) {
    println!("<{error}>");
}

fn get_json_string(filename: &str) -> Result<String, Box<dyn Error>> {
    let path = Path::new(filename);
    let json = fs::read_to_string(path).with_context(|| format!("could not read file `{}`", path.to_string_lossy()))?;

    Ok(json)
}

fn save_json(filename: &str, content: &str) -> Result<(), Box<dyn Error>> {
    let path = Path::new(filename);
    fs::write(path, content).with_context(|| format!("could not write file `{}`", path.to_string_lossy()))?;

    Ok(())
}



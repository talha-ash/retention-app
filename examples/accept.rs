use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args();
    let _argv0 = args.next();
    let Some(path) = args.next() else {
        eprintln!("usage: accept <path>");
        return ExitCode::from(1);
    };
    match retention_popup::accept_path(Path::new(&path)) {
        Ok(import) => {
            let Some(note) = import.notes.first() else {
                eprintln!("no notes");
                return ExitCode::from(1);
            };
            println!("guid {}", note.guid);
            println!("deck {}", note.deck);
            println!("notes {}", import.notes.len());
            println!("scheduling-applied 0");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

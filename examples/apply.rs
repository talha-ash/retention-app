use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use retention_popup::{Library, NoteBody, accept_text, apply_text};

const STATE: &str = "target/apply-state.txt";

fn main() -> ExitCode {
    let mut args = env::args();
    let _argv0 = args.next();
    let Some(path) = args.next() else {
        eprintln!("usage: apply <path>");
        return ExitCode::from(1);
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    let mut library = match load() {
        Ok(library) => library,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    let report = match apply_text(&mut library, &text) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };
    if let Err(err) = save(&library) {
        eprintln!("{err}");
        return ExitCode::from(1);
    }
    if report.added > 0 || report.retired > 0 {
        println!("added {}", report.added);
        println!("retired {}", report.retired);
    } else {
        println!("changed {}", report.changed);
    }
    ExitCode::SUCCESS
}

fn load() -> Result<Library, String> {
    if !Path::new(STATE).exists() {
        let import =
            accept_text(include_str!("../fixtures/basic.txt")).map_err(|err| err.to_string())?;
        let mut library = Library::new();
        let Some(note) = import.notes.into_iter().next() else {
            return Err("basic fixture has no note".to_string());
        };
        library.insert(note, Some("2026-11-20"));
        return Ok(library);
    }
    let text = fs::read_to_string(STATE).map_err(|err| err.to_string())?;
    let mut dues = std::collections::BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("#due:") else {
            continue;
        };
        let Some((guid, due)) = rest.split_once('\t') else {
            continue;
        };
        if !guid.is_empty() && !due.is_empty() {
            dues.insert(guid.to_string(), due.to_string());
        }
    }
    let import = accept_text(&text).map_err(|err| err.to_string())?;
    let mut library = Library::new();
    for note in import.notes {
        let due = dues.get(note.guid.as_str()).map(String::as_str);
        library.insert(note, due);
    }
    Ok(library)
}

fn save(library: &Library) -> Result<(), String> {
    let mut out = String::new();
    for note in library.notes() {
        if let Some(due) = library.due(note.guid.as_str()) {
            out.push_str("#due:");
            out.push_str(note.guid.as_str());
            out.push('\t');
            out.push_str(due);
            out.push('\n');
        }
    }
    out.push_str("#separator:Tab\n#notetype:Basic\n");
    out.push_str("#columns:Front\tBack\tguid\tdeck\n#guid column:3\n#deck column:4\n");
    for note in library.notes() {
        let NoteBody::Basic { front, back } = &note.body else {
            return Err("apply example stores Basic notes".to_string());
        };
        out.push_str(front);
        out.push('\t');
        out.push_str(back);
        out.push('\t');
        out.push_str(note.guid.as_str());
        out.push('\t');
        out.push_str(note.deck.as_str());
        out.push('\n');
    }
    if let Some(parent) = Path::new(STATE).parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let tmp = format!("{STATE}.tmp");
    fs::write(&tmp, out).map_err(|err| err.to_string())?;
    fs::rename(&tmp, STATE).map_err(|err| err.to_string())
}

use std::env;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use retention_popup::{accept_apkg, export_apkg, import_apkg, import_text, open_root};

fn main() -> ExitCode {
    let mut args = env::args();
    let _argv0 = args.next();
    let Some(path) = args.next() else {
        eprintln!("usage: roundtrip <path>");
        return ExitCode::from(1);
    };
    if path == "--cards" {
        let count = args
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1000);
        return match many_cards(count) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("{err}");
                ExitCode::from(1)
            }
        };
    }
    if let Err(err) = run(&path) {
        eprintln!("{err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn many_cards(count: usize) -> Result<(), String> {
    let root = open_root(&data_dir(), "probe").map_err(|err| err.to_string())?;
    let mut text = String::from(
        "#separator:Tab\n#html:false\n#notetype:Basic\n#deck:Spanish::Verbs\n#columns:Front\tBack\tguid\n#guid column:3\n",
    );
    for index in 0..count {
        text.push_str(&format!("front {index}\tback {index}\tn{index:04}\n"));
    }
    let started = Instant::now();
    let report = import_text(&root, &text).map_err(|err| err.to_string())?;
    println!(
        "{:.6} {} {count}",
        started.elapsed().as_secs_f64(),
        report.added
    );
    Ok(())
}

fn data_dir() -> PathBuf {
    if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("retention");
    }
    let home = env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local/share/retention")
}

fn run(path: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let root = open_root(&data_dir(), "basic").map_err(|err| err.to_string())?;
    if Path::new(path).extension().and_then(|ext| ext.to_str()) == Some("apkg") {
        import_apkg(&root, &bytes).map_err(|err| err.to_string())?;
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|_| "file is not utf-8".to_string())?;
        import_text(&root, text).map_err(|err| err.to_string())?;
    }
    let apkg = export_apkg(&root).map_err(|err| err.to_string())?;
    fs::write("export.apkg", &apkg).map_err(|err| err.to_string())?;
    let notes = accept_apkg(&apkg).map_err(|err| err.to_string())?;
    println!("exported {}", notes.notes.len());
    println!("fsrs-rows-in-apkg {}", fsrs_rows(&apkg)?);
    Ok(())
}

fn fsrs_rows(bytes: &[u8]) -> Result<u32, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|err| err.to_string())?;
    let mut total = 0u32;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|err| err.to_string())?;
        if file.name() != "memory.sqlite" {
            continue;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|err| err.to_string())?;
        total += count_memory(&buf)?;
    }
    Ok(total)
}

fn count_memory(bytes: &[u8]) -> Result<u32, String> {
    let mut conn = rusqlite::Connection::open_in_memory().map_err(|err| err.to_string())?;
    conn.deserialize_read_exact(rusqlite::MAIN_DB, Cursor::new(bytes), bytes.len(), true)
        .map_err(|err| err.to_string())?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;
    u32::try_from(count).map_err(|_| "fsrs row count does not fit".to_string())
}

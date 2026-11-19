use std::env;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use retention_popup::Grade;
use retention_popup::schedule;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    if args.next().as_deref() == Some("--repeat") {
        let count = args
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1000);
        return repeat(count);
    }
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs() as i64;
    for (name, grade) in [
        ("Again", Grade::Again),
        ("Hard", Grade::Hard),
        ("Good", Grade::Good),
        ("Easy", Grade::Easy),
    ] {
        match schedule(None, 0, grade, now_unix) {
            Ok(scheduled) => println!("{name} {}", scheduled.due_unix),
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}

fn repeat(count: u32) -> ExitCode {
    let started = Instant::now();
    let mut last = 0;
    for _ in 0..count {
        match schedule(None, 0, Grade::Good, 1_700_000_000) {
            Ok(scheduled) => last = scheduled.due_unix,
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::from(1);
            }
        }
    }
    println!("{:.6} {last} {count}", started.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

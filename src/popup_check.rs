//! Exhaustive check of the popup countdown in `App::update`.
//!
//! The reachable states are every sequence of the messages the control
//! card and the quest card can send, plus `Tick`, `Glow`, and `Quit`.
//! `Quit` only builds an exit task. This module never runs that task.

use super::{App, Corner, Message};

#[derive(Clone, Copy)]
enum Step {
    Delay(u64),
    Pick(Corner),
    Arm,
    Tick,
    Answer(&'static str),
    Close,
    Glow,
    Quit,
    Remove,
}

fn steps() -> Vec<Step> {
    vec![
        Step::Delay(5),
        Step::Delay(10),
        Step::Delay(30),
        Step::Pick(Corner::TopLeft),
        Step::Pick(Corner::TopRight),
        Step::Pick(Corner::BottomLeft),
        Step::Pick(Corner::BottomRight),
        Step::Arm,
        Step::Tick,
        Step::Answer("right corner"),
        Step::Answer("wrong place"),
        Step::Close,
        Step::Glow,
        Step::Quit,
        Step::Remove,
    ]
}

fn label(step: Step) -> String {
    match step {
        Step::Delay(secs) => format!("Delay({secs})"),
        Step::Pick(corner) => format!("Pick({})", corner.label()),
        Step::Arm => "Arm".to_string(),
        Step::Tick => "Tick".to_string(),
        Step::Answer(text) => format!("Answer({text})"),
        Step::Close => "ClosePopup".to_string(),
        Step::Glow => "Glow".to_string(),
        Step::Quit => "Quit".to_string(),
        Step::Remove => "RemoveWindow".to_string(),
    }
}

fn apply(app: &mut App, step: Step) {
    let message = match step {
        Step::Delay(secs) => Message::Delay(secs),
        Step::Pick(corner) => Message::Pick(corner),
        Step::Arm => Message::Arm,
        Step::Tick => Message::Tick,
        Step::Answer(text) => Message::Answer(text),
        Step::Close => Message::ClosePopup,
        Step::Glow => Message::Glow,
        Step::Quit => Message::Quit,
        Step::Remove => Message::RemoveWindow(app.popup_id.unwrap_or_else(iced::window::Id::unique)),
    };
    let _task = app.update(message);
}

fn dup(app: &App) -> App {
    App {
        delay_secs: app.delay_secs,
        corner: app.corner,
        remaining: app.remaining,
        popup_id: app.popup_id,
        answer: app.answer,
        opened_at: app.opened_at,
        smoke_time: app.smoke_time,
    }
}

fn corner_tag(corner: Corner) -> u8 {
    match corner {
        Corner::TopLeft => 0,
        Corner::TopRight => 1,
        Corner::BottomLeft => 2,
        Corner::BottomRight => 3,
    }
}

fn answer_tag(answer: Option<&str>) -> u8 {
    match answer {
        None => 0,
        Some("right corner") => 1,
        Some("wrong place") => 2,
        Some(_) => 9,
    }
}

fn key(app: &App) -> (u64, u8, Option<u64>, bool, u8) {
    (
        app.delay_secs,
        corner_tag(app.corner),
        app.remaining,
        app.popup_id.is_some(),
        answer_tag(app.answer),
    )
}

fn dump(app: &App, trace: &[String]) -> String {
    format!(
        "delay={} corner={} remaining={:?} popup={} answer={:?}\ntrace: {}",
        app.delay_secs,
        app.corner.label(),
        app.remaining,
        app.popup_id.is_some(),
        app.answer,
        if trace.is_empty() {
            "(start)".to_string()
        } else {
            trace.join(" -> ")
        }
    )
}

fn fail(property: &str, app: &App, trace: &[String]) -> String {
    format!("{property} failed\n{}", dump(app, trace))
}

fn assert_state(app: &App, trace: &[String]) {
    assert!(
        app.popup_id.is_none() || app.remaining.is_none(),
        "{}",
        fail("countdown_and_card_are_exclusive", app, trace)
    );
    assert!(
        matches!(app.delay_secs, 5 | 10 | 30),
        "{}",
        fail("delay_stays_on_a_chip", app, trace)
    );
    assert!(
        matches!(app.remaining, None | Some(1..=30)),
        "{}",
        fail("countdown_stays_in_1_to_30", app, trace)
    );
    assert!(
        answer_tag(app.answer) != 9,
        "{}",
        fail("answer_records_only_the_pressed_path", app, trace)
    );
}

fn assert_opens_on_nth_tick(start: &App, trace: &[String]) {
    let mut app = dup(start);
    let mut trace = trace.to_vec();
    let delay = app.delay_secs;
    apply(&mut app, Step::Arm);
    trace.push("Arm".to_string());
    assert!(
        app.popup_id.is_none() && app.remaining == Some(delay) && app.answer.is_none(),
        "{}",
        fail("arm_clears_the_card_and_starts_the_chosen_delay", &app, &trace)
    );
    for done in 1..delay {
        apply(&mut app, Step::Tick);
        trace.push("Tick".to_string());
        assert!(
            app.popup_id.is_none() && app.remaining == Some(delay - done),
            "{}",
            fail("card_opens_on_the_nth_tick", &app, &trace)
        );
    }
    apply(&mut app, Step::Tick);
    trace.push("Tick".to_string());
    assert!(
        app.popup_id.is_some() && app.remaining.is_none(),
        "{}",
        fail("card_opens_on_the_nth_tick", &app, &trace)
    );
    let open = app.popup_id;
    apply(&mut app, Step::Tick);
    trace.push("Tick".to_string());
    assert!(
        app.popup_id == open && app.remaining.is_none(),
        "{}",
        fail("a_tick_after_open_does_not_open_another_card", &app, &trace)
    );
}

fn assert_delay_chip(start: &App, trace: &[String]) {
    let mut app = dup(start);
    let mut trace = trace.to_vec();
    apply(&mut app, Step::Arm);
    trace.push("Arm".to_string());
    let left = app.remaining;
    let other = if app.delay_secs == 5 { 30 } else { 5 };
    apply(&mut app, Step::Delay(other));
    trace.push(label(Step::Delay(other)));
    assert!(
        app.remaining == left && app.delay_secs == other && app.popup_id.is_none(),
        "{}",
        fail("delay_chip_does_not_change_a_running_countdown", &app, &trace)
    );
    apply(&mut app, Step::Arm);
    trace.push("Arm".to_string());
    assert!(
        app.remaining == Some(other),
        "{}",
        fail("the_next_show_card_uses_the_new_delay", &app, &trace)
    );
}

fn assert_close_and_answer(start: &App, trace: &[String]) {
    let mut app = dup(start);
    let mut trace = trace.to_vec();
    apply(&mut app, Step::Arm);
    trace.push("Arm".to_string());
    for _ in 0..app.delay_secs {
        apply(&mut app, Step::Tick);
        trace.push("Tick".to_string());
    }
    assert!(app.popup_id.is_some(), "{}", fail("card_opens_on_the_nth_tick", &app, &trace));
    apply(&mut app, Step::Answer("right corner"));
    trace.push("Answer(right corner)".to_string());
    assert!(
        app.answer == Some("right corner") && app.popup_id.is_some() && app.remaining.is_none(),
        "{}",
        fail("answer_records_only_the_pressed_path", &app, &trace)
    );
    apply(&mut app, Step::Answer("wrong place"));
    trace.push("Answer(wrong place)".to_string());
    assert!(
        app.answer == Some("wrong place") && app.popup_id.is_some(),
        "{}",
        fail("answer_records_only_the_pressed_path", &app, &trace)
    );
    let answer = app.answer;
    apply(&mut app, Step::Close);
    trace.push("ClosePopup".to_string());
    assert!(
        app.popup_id.is_none() && app.answer == answer && app.remaining.is_none(),
        "{}",
        fail("close_removes_the_card", &app, &trace)
    );
}

#[test]
fn explore_popup_machine() {
    let mut states = vec![App::new()];
    let mut traces: Vec<Vec<String>> = vec![Vec::new()];
    let mut seen = std::collections::HashSet::new();
    seen.insert(key(&states[0]));

    let mut index = 0;
    while index < states.len() {
        for step in steps() {
            let mut next = dup(&states[index]);
            let before = key(&next);
            apply(&mut next, step);
            let mut trace = traces[index].clone();
            trace.push(label(step));
            if step_is_quit(step) {
                assert!(
                    key(&next) == before,
                    "{}",
                    fail("quit_leaves_the_card_state_unchanged", &next, &trace)
                );
            }
            assert_state(&next, &trace);
            if seen.insert(key(&next)) {
                states.push(next);
                traces.push(trace);
            }
        }
        index += 1;
    }

    for (state, trace) in states.iter().zip(traces.iter()) {
        assert_state(state, trace);
        assert_opens_on_nth_tick(state, trace);
        assert_delay_chip(state, trace);
        assert_close_and_answer(state, trace);
    }

    assert!(
        states.len() > 1,
        "the explorer never left the start state"
    );
    eprintln!("popup machine reachable states: {}", states.len());
}

fn step_is_quit(step: Step) -> bool {
    matches!(step, Step::Quit)
}

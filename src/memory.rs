#[derive(Debug, PartialEq)]
pub enum Grade {
    Again,
    Hard,
    Good,
    Easy,
}

pub struct Scheduled {
    pub memory: fsrs::MemoryState,
    pub due_unix: i64,
    pub log: fsrs::FSRSReview,
}

pub fn schedule(
    previous: Option<fsrs::MemoryState>,
    elapsed_days: u32,
    grade: Grade,
    now_unix: i64,
) -> Result<Scheduled, fsrs::FSRSError> {
    let states = fsrs::FSRS::default().next_states(previous, 0.90, elapsed_days)?;
    let (item, rating) = match grade {
        Grade::Again => (states.again, 1),
        Grade::Hard => (states.hard, 2),
        Grade::Good => (states.good, 3),
        Grade::Easy => (states.easy, 4),
    };
    Ok(Scheduled {
        memory: item.memory,
        due_unix: now_unix + (item.interval.round().max(1.0) as i64) * 86400,
        log: fsrs::FSRSReview {
            rating,
            delta_t: elapsed_days as f32,
        },
    })
}

pub fn grade_choice(selected: &[&str], correct: &[&str]) -> Grade {
    let same = selected.iter().all(|id| correct.contains(id))
        && correct.iter().all(|id| selected.contains(id));
    if same { Grade::Good } else { Grade::Again }
}

#[cfg(test)]
mod tests {
    use fsrs::FSRS;

    use super::Grade;
    use super::grade_choice;
    use super::schedule;

    #[test]
    fn schedule_new_card_returns_next_states_memory_and_due() {
        let now_unix = 1_700_000_000;
        let elapsed_days = 0;
        let states = FSRS::default()
            .next_states(None, 0.90, elapsed_days)
            .expect("next_states");
        let cases = [
            (Grade::Again, &states.again, 1),
            (Grade::Hard, &states.hard, 2),
            (Grade::Good, &states.good, 3),
            (Grade::Easy, &states.easy, 4),
        ];
        for (grade, item, rating) in cases {
            let scheduled = schedule(None, elapsed_days, grade, now_unix).expect("schedule");
            let due_unix = now_unix + (item.interval.round().max(1.0) as i64) * 86400;
            assert_eq!(scheduled.memory, item.memory);
            assert_eq!(scheduled.due_unix, due_unix);
            assert_eq!(scheduled.log.rating, rating);
            assert_eq!(scheduled.log.delta_t, 0.0);
        }
    }

    #[test]
    fn grade_choice_same_ids_returns_good() {
        assert_eq!(
            grade_choice(&["good", "again"], &["again", "good"]),
            Grade::Good
        );
    }

    #[test]
    fn grade_choice_missing_id_returns_again() {
        assert_eq!(grade_choice(&["again"], &["again", "good"]), Grade::Again);
    }

    #[test]
    fn grade_choice_extra_id_returns_again() {
        assert_eq!(
            grade_choice(&["again", "good", "easy"], &["again", "good"]),
            Grade::Again
        );
    }
}

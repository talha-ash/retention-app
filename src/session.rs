use retention_popup::Grade;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unix(i64);

impl Unix {
    pub const fn new(seconds: i64) -> Self {
        Self(seconds)
    }

    pub const fn get(self) -> i64 {
        self.0
    }

    fn saturating_add(self, gap: Gap) -> Self {
        Self(self.0.saturating_add(gap.seconds()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Gap(i64);

impl Gap {
    pub const fn from_seconds(seconds: i64) -> Option<Self> {
        if seconds < 0 {
            None
        } else {
            Some(Self(seconds))
        }
    }

    pub const fn seconds(self) -> i64 {
        self.0
    }
}

const MINUTES_PER_DAY: i32 = 24 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Minute(i32);

impl Minute {
    pub const fn new(minute: i32) -> Option<Self> {
        if minute >= 0 && minute < MINUTES_PER_DAY {
            Some(Self(minute))
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub const fn get(self) -> i32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    start: i32,
    end: i32,
}

impl Window {
    pub const fn new(start: i32, end: i32) -> Option<Self> {
        if start < 0 || start >= end || end > MINUTES_PER_DAY {
            None
        } else {
            Some(Self { start, end })
        }
    }

    pub const fn contains(self, minute: Minute) -> bool {
        minute.0 >= self.start && minute.0 < self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Guid(String);

impl Guid {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CardOrd(i64);

impl CardOrd {
    pub const fn new(value: i64) -> Option<Self> {
        if value < 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeckName(String);

impl DeckName {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    guid: Guid,
    ord: CardOrd,
    due_unix: Unix,
    last_grade: Option<Grade>,
}

impl Card {
    pub fn new(guid: Guid, ord: CardOrd, due_unix: Unix) -> Self {
        Self {
            guid,
            ord,
            due_unix,
            last_grade: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Deck {
    name: DeckName,
    window: Window,
    gap: Gap,
    next_allowed: Unix,
    cards: Vec<Card>,
}

impl Deck {
    pub fn new(
        name: DeckName,
        window: Window,
        gap: Gap,
        next_allowed: Unix,
        cards: Vec<Card>,
    ) -> Option<Self> {
        if !distinct_cards(&cards) {
            return None;
        }
        Some(Self {
            name,
            window,
            gap,
            next_allowed,
            cards,
        })
    }
}

fn distinct_cards(cards: &[Card]) -> bool {
    cards.iter().enumerate().all(|(index, card)| {
        cards[index + 1..]
            .iter()
            .all(|other| other.guid != card.guid || other.ord != card.ord)
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    decks: Vec<Deck>,
}

impl Session {
    pub fn new(decks: Vec<Deck>) -> Option<Self> {
        let distinct = decks.iter().enumerate().all(|(index, deck)| {
            decks[index + 1..]
                .iter()
                .all(|other| other.name != deck.name)
        });
        if distinct {
            Some(Self { decks })
        } else {
            None
        }
    }

    pub fn pick(&self, now: Unix, minute: Minute) -> Option<Picked> {
        let mut best: Option<(&Deck, &Card)> = None;
        for deck in &self.decks {
            if !deck.window.contains(minute) {
                continue;
            }
            if now < deck.next_allowed {
                continue;
            }
            for card in &deck.cards {
                if card.due_unix > now {
                    continue;
                }
                let take = match best {
                    None => true,
                    Some((_, other)) => {
                        (&card.due_unix, &card.guid, card.ord)
                            < (&other.due_unix, &other.guid, other.ord)
                    }
                };
                if take {
                    best = Some((deck, card));
                }
            }
        }
        best.map(|(deck, card)| Picked {
            deck: deck.name.clone(),
            guid: card.guid.clone(),
            ord: card.ord,
            due_unix: card.due_unix,
        })
    }

    #[must_use]
    pub fn answer(&mut self, deck: &DeckName, guid: &Guid, ord: CardOrd, review: Review) -> bool {
        let Some(index) = self.decks.iter().position(|item| &item.name == deck) else {
            return false;
        };
        let next_allowed = review.now.saturating_add(self.decks[index].gap);
        let Some(card_at) = self.decks[index]
            .cards
            .iter()
            .position(|card| &card.guid == guid && card.ord == ord)
        else {
            return false;
        };
        let card = &mut self.decks[index].cards[card_at];
        card.due_unix = review.due_unix;
        card.last_grade = Some(review.grade);
        self.decks[index].next_allowed = next_allowed;
        true
    }

    pub fn next_allowed(&self, deck: &DeckName) -> Option<Unix> {
        self.decks
            .iter()
            .find(|item| &item.name == deck)
            .map(|item| item.next_allowed)
    }

    #[allow(dead_code)]
    pub fn due_unix(&self, deck: &DeckName, guid: &Guid, ord: CardOrd) -> Option<Unix> {
        self.card(deck, guid, ord).map(|card| card.due_unix)
    }

    #[allow(dead_code)]
    pub fn last_grade(&self, deck: &DeckName, guid: &Guid, ord: CardOrd) -> Option<Grade> {
        self.card(deck, guid, ord).and_then(|card| card.last_grade)
    }

    #[allow(dead_code)]
    fn card(&self, deck: &DeckName, guid: &Guid, ord: CardOrd) -> Option<&Card> {
        self.decks
            .iter()
            .find(|item| &item.name == deck)
            .and_then(|item| {
                item.cards
                    .iter()
                    .find(|card| &card.guid == guid && card.ord == ord)
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
    pub deck: DeckName,
    pub guid: Guid,
    pub ord: CardOrd,
    pub due_unix: Unix,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Review {
    pub grade: Grade,
    pub now: Unix,
    pub due_unix: Unix,
}

#[cfg(test)]
mod tests {
    use retention_popup::Grade;

    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn open_day() -> Window {
        Window::new(9 * 60, 18 * 60).expect("window")
    }

    fn minute_at(minute: i32) -> Minute {
        Minute::new(minute).expect("minute")
    }

    fn deck_with(name: &str, guid: &str, ord: i64, gap_secs: i64, due: i64) -> Deck {
        Deck::new(
            DeckName::new(name).expect("deck"),
            open_day(),
            Gap::from_seconds(gap_secs).expect("gap"),
            Unix::new(0),
            vec![Card::new(
                Guid::new(guid).expect("guid"),
                CardOrd::new(ord).expect("ord"),
                Unix::new(due),
            )],
        )
        .expect("cards")
    }

    fn named(name: &str) -> DeckName {
        DeckName::new(name).expect("deck")
    }

    fn guid(value: &str) -> Guid {
        Guid::new(value).expect("guid")
    }

    fn ord(value: i64) -> CardOrd {
        CardOrd::new(value).expect("ord")
    }

    #[test]
    fn again_moves_next_allowed_by_the_gap() {
        for gap_secs in [5, 2700] {
            let focus = named("focus");
            let other = named("other");
            let card = guid("card");
            let card_ord = ord(0);
            let mut session = Session::new(vec![
                deck_with("focus", "card", 0, gap_secs, 0),
                deck_with("other", "else", 0, 5, 0),
            ])
            .expect("session");
            let applied = session.answer(
                &focus,
                &card,
                card_ord,
                Review {
                    grade: Grade::Again,
                    now: Unix::new(NOW),
                    due_unix: Unix::new(NOW + 3),
                },
            );
            assert!(applied);
            assert_eq!(
                session.next_allowed(&focus),
                Some(Unix::new(NOW + gap_secs))
            );
            assert_eq!(session.next_allowed(&other), Some(Unix::new(0)));
            assert_eq!(
                session.due_unix(&focus, &card, card_ord),
                Some(Unix::new(NOW + 3))
            );
            assert_eq!(
                session.last_grade(&focus, &card, card_ord),
                Some(Grade::Again)
            );
        }
    }

    #[test]
    fn one_card_when_two_decks_are_ready() {
        let now = Unix::new(1_000);
        let minute = minute_at(10 * 60);

        let earlier_due = Session::new(vec![
            deck_with("first", "a", 0, 5, 50),
            deck_with("second", "m", 0, 5, 0),
        ])
        .expect("session");
        assert_eq!(
            earlier_due.pick(now, minute),
            Some(Picked {
                deck: named("second"),
                guid: guid("m"),
                ord: ord(0),
                due_unix: Unix::new(0),
            })
        );

        let guid_tie = Session::new(vec![
            deck_with("first", "b", 0, 5, 10),
            deck_with("second", "a", 0, 5, 10),
        ])
        .expect("session");
        assert_eq!(
            guid_tie.pick(now, minute),
            Some(Picked {
                deck: named("second"),
                guid: guid("a"),
                ord: ord(0),
                due_unix: Unix::new(10),
            })
        );

        let ord_tie = Session::new(vec![
            deck_with("first", "a", 2, 5, 10),
            deck_with("second", "a", 0, 5, 10),
        ])
        .expect("session");
        assert_eq!(
            ord_tie.pick(now, minute),
            Some(Picked {
                deck: named("second"),
                guid: guid("a"),
                ord: ord(0),
                due_unix: Unix::new(10),
            })
        );
    }

    #[test]
    fn hidden_inside_the_gap() {
        let deck = named("focus");
        let card = guid("card");
        let card_ord = ord(0);
        let minute = minute_at(10 * 60);
        let mut session = Session::new(vec![deck_with("focus", "card", 0, 5, 0)]).expect("session");
        assert_eq!(
            session.pick(Unix::new(NOW), minute).expect("due").guid,
            card
        );
        assert!(session.answer(
            &deck,
            &card,
            card_ord,
            Review {
                grade: Grade::Again,
                now: Unix::new(NOW),
                due_unix: Unix::new(NOW + 3),
            },
        ));
        let hidden_at = NOW + 3;
        assert!(
            session
                .due_unix(&deck, &card, card_ord)
                .expect("stored due")
                .get()
                <= hidden_at
        );
        assert_eq!(session.pick(Unix::new(hidden_at), minute), None);
        assert_eq!(
            session
                .pick(Unix::new(NOW + 5), minute)
                .expect("after gap")
                .guid,
            card
        );
    }

    #[test]
    fn deck_outside_its_window_contributes_nothing() {
        let session = Session::new(vec![deck_with("focus", "card", 0, 5, 0)]).expect("session");
        let now = Unix::new(1_000);
        assert_eq!(session.pick(now, minute_at(8 * 60)), None);
        assert!(session.pick(now, minute_at(9 * 60)).is_some());
        assert_eq!(session.pick(now, minute_at(18 * 60)), None);
    }

    #[test]
    fn rejects_values_that_are_not_a_session() {
        assert!(Guid::new("").is_none());
        assert!(DeckName::new("").is_none());
        assert!(CardOrd::new(-1).is_none());
        assert!(Gap::from_seconds(-1).is_none());
        assert!(Gap::from_seconds(0).is_some());
        assert!(Minute::new(-1).is_none());
        assert!(Minute::new(24 * 60).is_none());
        assert!(Minute::new(0).is_some());
        assert!(Window::new(18 * 60, 9 * 60).is_none());
        assert!(Window::new(0, 24 * 60).is_some());
        assert!(Window::new(0, 24 * 60 + 1).is_none());

        let shared = Guid::new("a").expect("guid");
        let duplicate_cards = Deck::new(
            named("focus"),
            open_day(),
            Gap::from_seconds(5).expect("gap"),
            Unix::new(0),
            vec![
                Card::new(shared.clone(), ord(0), Unix::new(0)),
                Card::new(shared.clone(), ord(0), Unix::new(1)),
            ],
        );
        assert!(duplicate_cards.is_none());

        let two_ords = Deck::new(
            named("focus"),
            open_day(),
            Gap::from_seconds(5).expect("gap"),
            Unix::new(0),
            vec![
                Card::new(shared.clone(), ord(0), Unix::new(0)),
                Card::new(shared, ord(1), Unix::new(0)),
            ],
        )
        .expect("two cards of one note");
        let same_name = deck_with("focus", "else", 0, 5, 0);
        assert!(Session::new(vec![two_ords, same_name]).is_none());
    }
}

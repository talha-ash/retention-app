const MINUTES_PER_DAY: i32 = 24 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    start: i32,
    end: i32,
}

impl Window {
    pub fn new(start: i32, end: i32) -> Option<Self> {
        if start >= end || end > MINUTES_PER_DAY {
            None
        } else {
            Some(Self { start, end })
        }
    }

    fn contains(self, minute: i32) -> bool {
        minute >= self.start && minute < self.end
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gap(i32);

impl Gap {
    pub fn new(minutes: i32) -> Option<Self> {
        if minutes < 1 {
            None
        } else {
            Some(Self(minutes))
        }
    }

    pub fn minutes(self) -> i32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub window: Window,
    pub gap: Gap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeckDelivery {
    Inherit,
    Override(Delivery),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueCard {
    pub name: String,
    pub deck: String,
    pub due_minute: i32,
    pub learning_left: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Show {
    pub minute: i32,
    pub deck: String,
    pub card: String,
}

pub fn per_deck_shows(
    cards: &mut [DueCard],
    decks: &[(&str, DeckDelivery)],
    root: &Delivery,
) -> Vec<Show> {
    shows_at(cards, decks, root, 0..MINUTES_PER_DAY)
}

fn shows_at(
    cards: &mut [DueCard],
    decks: &[(&str, DeckDelivery)],
    root: &Delivery,
    minutes: impl IntoIterator<Item = i32>,
) -> Vec<Show> {
    let mut ready = vec![0; decks.len()];
    let mut shows = Vec::new();
    for minute in minutes {
        let mut chosen: Option<(usize, usize)> = None;
        for (index, card) in cards.iter().enumerate() {
            let Some(deck_at) = decks.iter().position(|(name, _)| *name == card.deck) else {
                continue;
            };
            let rules = rules_of(decks[deck_at].1, root);
            if !rules.window.contains(minute) || minute < ready[deck_at] || card.due_minute > minute
            {
                continue;
            }
            let better = match chosen {
                None => true,
                Some((at, _)) => {
                    let other = &cards[at];
                    (card.due_minute, card.name.as_str()) < (other.due_minute, other.name.as_str())
                }
            };
            if better {
                chosen = Some((index, deck_at));
            }
        }
        let Some((index, deck_at)) = chosen else {
            continue;
        };
        let rules = rules_of(decks[deck_at].1, root);
        shows.push(Show {
            minute,
            deck: cards[index].deck.clone(),
            card: cards[index].name.clone(),
        });
        let card = &mut cards[index];
        if card.learning_left > 0 {
            card.learning_left -= 1;
            card.due_minute = minute + 10;
        } else {
            card.due_minute = minute + MINUTES_PER_DAY;
        }
        ready[deck_at] = minute + rules.gap.minutes();
    }
    shows
}

fn rules_of(choice: DeckDelivery, root: &Delivery) -> Delivery {
    match choice {
        DeckDelivery::Inherit => *root,
        DeckDelivery::Override(delivery) => delivery,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_window() -> Window {
        Window::new(9 * 60, 18 * 60).expect("window")
    }

    fn recorded_cards() -> [DueCard; 4] {
        [
            DueCard {
                name: "early".to_string(),
                deck: "vocab".to_string(),
                due_minute: 8 * 60 + 30,
                learning_left: 0,
            },
            DueCard {
                name: "hola".to_string(),
                deck: "vocab".to_string(),
                due_minute: 9 * 60,
                learning_left: 0,
            },
            DueCard {
                name: "hola-2".to_string(),
                deck: "vocab".to_string(),
                due_minute: 9 * 60,
                learning_left: 0,
            },
            DueCard {
                name: "layer".to_string(),
                deck: "hot".to_string(),
                due_minute: 9 * 60 + 10,
                learning_left: 2,
            },
        ]
    }

    #[test]
    fn per_deck_recorded_fixture() {
        let window = open_window();
        let root = Delivery {
            window,
            gap: Gap::new(45).expect("gap"),
        };
        let decks = [
            (
                "vocab",
                DeckDelivery::Override(Delivery {
                    window,
                    gap: Gap::new(45).expect("gap"),
                }),
            ),
            (
                "hot",
                DeckDelivery::Override(Delivery {
                    window,
                    gap: Gap::new(10).expect("gap"),
                }),
            ),
        ];
        let mut cards = recorded_cards();
        let shows = per_deck_shows(&mut cards, &decks, &root);
        assert_eq!(
            shows,
            [
                Show {
                    minute: 9 * 60,
                    deck: "vocab".to_string(),
                    card: "early".to_string(),
                },
                Show {
                    minute: 9 * 60 + 10,
                    deck: "hot".to_string(),
                    card: "layer".to_string(),
                },
                Show {
                    minute: 9 * 60 + 20,
                    deck: "hot".to_string(),
                    card: "layer".to_string(),
                },
                Show {
                    minute: 9 * 60 + 30,
                    deck: "hot".to_string(),
                    card: "layer".to_string(),
                },
                Show {
                    minute: 9 * 60 + 45,
                    deck: "vocab".to_string(),
                    card: "hola".to_string(),
                },
                Show {
                    minute: 10 * 60 + 30,
                    deck: "vocab".to_string(),
                    card: "hola-2".to_string(),
                },
            ]
        );
    }

    #[test]
    fn per_deck_inherit_uses_root_gap() {
        let window = open_window();
        let root = Delivery {
            window,
            gap: Gap::new(45).expect("gap"),
        };
        let decks = [("vocab", DeckDelivery::Inherit)];
        let mut cards = [
            DueCard {
                name: "early".to_string(),
                deck: "vocab".to_string(),
                due_minute: 8 * 60 + 30,
                learning_left: 0,
            },
            DueCard {
                name: "hola".to_string(),
                deck: "vocab".to_string(),
                due_minute: 9 * 60,
                learning_left: 0,
            },
        ];
        let shows = per_deck_shows(&mut cards, &decks, &root);
        assert_eq!(
            shows,
            [
                Show {
                    minute: 9 * 60,
                    deck: "vocab".to_string(),
                    card: "early".to_string(),
                },
                Show {
                    minute: 9 * 60 + 45,
                    deck: "vocab".to_string(),
                    card: "hola".to_string(),
                },
            ]
        );
        assert_eq!(Gap::new(0), None);
        assert_eq!(Window::new(9 * 60, 9 * 60), None);
    }

    #[test]
    fn per_deck_one_card_per_minute() {
        let window = open_window();
        let gap = Gap::new(10).expect("gap");
        let root = Delivery { window, gap };
        let decks = [
            ("a", DeckDelivery::Override(Delivery { window, gap })),
            ("b", DeckDelivery::Override(Delivery { window, gap })),
        ];
        let mut cards = [
            DueCard {
                name: "alpha".to_string(),
                deck: "a".to_string(),
                due_minute: 9 * 60,
                learning_left: 0,
            },
            DueCard {
                name: "beta".to_string(),
                deck: "b".to_string(),
                due_minute: 9 * 60,
                learning_left: 0,
            },
        ];
        let shows = per_deck_shows(&mut cards, &decks, &root);
        assert_eq!(
            shows,
            [
                Show {
                    minute: 9 * 60,
                    deck: "a".to_string(),
                    card: "alpha".to_string(),
                },
                Show {
                    minute: 9 * 60 + 1,
                    deck: "b".to_string(),
                    card: "beta".to_string(),
                },
            ]
        );
    }

    #[test]
    fn per_deck_same_rules_at_nine_and_sixteen() {
        let window = open_window();
        let root = Delivery {
            window,
            gap: Gap::new(45).expect("gap"),
        };
        let decks = [
            (
                "vocab",
                DeckDelivery::Override(Delivery {
                    window,
                    gap: Gap::new(45).expect("gap"),
                }),
            ),
            (
                "hot",
                DeckDelivery::Override(Delivery {
                    window,
                    gap: Gap::new(10).expect("gap"),
                }),
            ),
        ];
        let mut cards = recorded_cards();
        let shows = shows_at(&mut cards, &decks, &root, [9 * 60, 16 * 60]);
        assert_eq!(
            shows,
            [
                Show {
                    minute: 9 * 60,
                    deck: "vocab".to_string(),
                    card: "early".to_string(),
                },
                Show {
                    minute: 16 * 60,
                    deck: "vocab".to_string(),
                    card: "hola".to_string(),
                },
            ]
        );
        assert!(shows.iter().all(|show| show.card != "layer"));
    }
}

use std::env;
use std::time::Instant;

use retention_popup::{DeckDelivery, Delivery, DueCard, Gap, Window, per_deck_shows};

fn main() {
    let mut args = env::args().skip(1);
    if args.next().as_deref() == Some("--cards") {
        let count = args
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(10_000);
        many_cards(count);
        return;
    }
    let window = Window::new(9 * 60, 18 * 60).expect("window");
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
    ];
    for show in per_deck_shows(&mut cards, &decks, &root) {
        println!(
            "{:02}:{:02} {}/{}",
            show.minute / 60,
            show.minute % 60,
            show.deck,
            show.card
        );
    }
}

fn many_cards(count: usize) {
    let window = Window::new(9 * 60, 18 * 60).expect("window");
    let root = Delivery {
        window,
        gap: Gap::new(45).expect("gap"),
    };
    let decks = [("vocab", DeckDelivery::Inherit)];
    let mut cards = Vec::with_capacity(count);
    for index in 0..count {
        cards.push(DueCard {
            name: format!("card-{index:05}"),
            deck: "vocab".to_string(),
            due_minute: 9 * 60,
            learning_left: 0,
        });
    }
    let started = Instant::now();
    let shows = per_deck_shows(&mut cards, &decks, &root);
    println!(
        "{:.6} {} {count}",
        started.elapsed().as_secs_f64(),
        shows.len()
    );
}

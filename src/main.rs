//! Corner-popup prototype. A centered control card waits, then a second
//! layer-shell card appears in the corner you picked.

mod smoke;

use std::time::{Duration, Instant};

use iced::widget::shader::Shader;
use iced::widget::{button, column, container, row, text, Stack};
use iced::gradient::Linear;
use iced::{
    Alignment, Background, Border, Color, Element, Length, Radians, Shadow, Subscription, Task,
    Theme, Vector,
};
use iced_layershell::build_pattern::daemon;
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

const BG: Color = Color::from_rgb8(0x1a, 0x1b, 0x26);
const CARD: Color = Color::from_rgb8(0x24, 0x28, 0x3b);
const FG: Color = Color::from_rgb8(0xc0, 0xca, 0xf5);
const MUTED: Color = Color::from_rgb8(0x56, 0x5f, 0x89);
const ACCENT: Color = Color::from_rgb8(0x7a, 0xa2, 0xf7);
const LINE: Color = Color::from_rgb8(0x41, 0x48, 0x68);
const GOLD: Color = Color::from_rgb8(0xe8, 0xc3, 0x72);
const GOLD_HOT: Color = Color::from_rgb8(0xff, 0xe1, 0xa8);
const INK: Color = Color::from_rgb8(0x1a, 0x12, 0x08);

const POPUP_SIZE: (u32, u32) = (652, 552);
const MARGIN: i32 = 16;

fn main() -> iced_layershell::Result {
    daemon(App::new, "retention-controls", App::update, App::view)
        .subscription(App::subscription)
        .theme(|_: &App, _: iced::window::Id| Theme::Dark)
        .style(|_, theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .settings(Settings {
            layer_settings: LayerShellSettings {
                // No edges: the compositor centers this one. The popup is the anchored card.
                anchor: Anchor::empty(),
                layer: Layer::Top,
                exclusive_zone: 0,
                size: Some((420, 520)),
                margin: (0, 0, 0, 0),
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                start_mode: StartMode::Active,
                events_transparent: false,
            },
            ..Default::default()
        })
        .antialiasing(false)
        .run()
}

struct App {
    delay_secs: u64,
    corner: Corner,
    /// Seconds left before the card opens. `None` means we are not waiting.
    remaining: Option<u64>,
    popup_id: Option<iced::window::Id>,
    answer: Option<&'static str>,
    opened_at: Option<Instant>,
    smoke_time: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
            Corner::BottomLeft => "Bottom left",
            Corner::BottomRight => "Bottom right",
        }
    }

    fn anchor(self) -> Anchor {
        match self {
            Corner::TopLeft => Anchor::Top | Anchor::Left,
            Corner::TopRight => Anchor::Top | Anchor::Right,
            Corner::BottomLeft => Anchor::Bottom | Anchor::Left,
            Corner::BottomRight => Anchor::Bottom | Anchor::Right,
        }
    }
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Delay(u64),
    Pick(Corner),
    Arm,
    Tick,
    Answer(&'static str),
    ClosePopup,
    Glow,
    Quit,
}

impl App {
    fn new() -> Self {
        Self {
            delay_secs: 5,
            corner: Corner::TopRight,
            remaining: None,
            popup_id: None,
            answer: None,
            opened_at: None,
            smoke_time: 0.0,
        }
    }

    fn subscription(state: &Self) -> Subscription<Message> {
        let countdown = state.remaining.is_some().then(|| {
            iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
        });
        let smoke = state.popup_id.is_some().then(|| {
            iced::time::every(Duration::from_millis(32)).map(|_| Message::Glow)
        });
        Subscription::batch(countdown.into_iter().chain(smoke))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Delay(secs) => {
                self.delay_secs = secs;
                Task::none()
            }
            Message::Pick(corner) => {
                self.corner = corner;
                Task::none()
            }
            Message::Arm => {
                self.remaining = Some(self.delay_secs);
                self.answer = None;
                self.opened_at = None;
                self.smoke_time = 0.0;
                match self.popup_id.take() {
                    Some(id) => Task::done(Message::RemoveWindow(id)),
                    None => Task::none(),
                }
            }
            Message::Tick => match self.remaining {
                Some(1) | Some(0) => {
                    self.remaining = None;
                    if self.popup_id.is_some() {
                        return Task::none();
                    }
                    let (id, task) = Message::layershell_open(NewLayerShellSettings {
                        size: Some(POPUP_SIZE),
                        layer: Layer::Top,
                        anchor: self.corner.anchor(),
                        exclusive_zone: Some(0),
                        margin: Some((MARGIN, MARGIN, MARGIN, MARGIN)),
                        keyboard_interactivity: KeyboardInteractivity::OnDemand,
                        output_option: OutputOption::Active,
                        events_transparent: false,
                        namespace: Some("retention-card".into()),
                    });
                    self.popup_id = Some(id);
                    self.opened_at = Some(Instant::now());
                    task
                }
                Some(secs) => {
                    self.remaining = Some(secs - 1);
                    Task::none()
                }
                None => Task::none(),
            },
            Message::Answer(label) => {
                self.answer = Some(label);
                Task::none()
            }
            Message::ClosePopup => {
                self.opened_at = None;
                self.smoke_time = 0.0;
                match self.popup_id.take() {
                    Some(id) => Task::done(Message::RemoveWindow(id)),
                    None => Task::none(),
                }
            }
            Message::Glow => {
                if let Some(start) = self.opened_at {
                    self.smoke_time = start.elapsed().as_secs_f32();
                }
                Task::none()
            }
            Message::Quit => iced::exit(),
            _ => Task::none(),
        }
    }

    fn view(&self, id: iced::window::Id) -> Element<'_, Message> {
        if self.popup_id == Some(id) {
            Stack::with_children([
                Shader::new(smoke::Smoke {
                    time: self.smoke_time,
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
                container(quest_card(self.popup()))
                    .padding(smoke::PAD)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            card(self.controls())
        }
    }

    fn controls(&self) -> Element<'_, Message> {
        let status = if self.popup_id.is_some() {
            "Card is open. Close it with ×.".to_string()
        } else if let Some(secs) = self.remaining {
            format!("Card appears in {secs}s, {}", self.corner.label().to_lowercase())
        } else {
            "Pick a delay and a corner, then show the card.".to_string()
        };

        let show = if self.remaining.is_some() {
            "Restart timer"
        } else {
            "Show card"
        };

        column![
            text("Popup test").size(28).color(FG),
            text("The card is its own window. This panel stays here.")
                .size(14)
                .color(MUTED),
            text("Delay").size(13).color(MUTED),
            row![
                chip("5 seconds", self.delay_secs == 5, Message::Delay(5)),
                chip("10 seconds", self.delay_secs == 10, Message::Delay(10)),
                chip("30 seconds", self.delay_secs == 30, Message::Delay(30)),
            ]
            .spacing(8),
            text("Corner").size(13).color(MUTED),
            row![
                chip(
                    Corner::TopLeft.label(),
                    self.corner == Corner::TopLeft,
                    Message::Pick(Corner::TopLeft),
                ),
                chip(
                    Corner::TopRight.label(),
                    self.corner == Corner::TopRight,
                    Message::Pick(Corner::TopRight),
                ),
            ]
            .spacing(8),
            row![
                chip(
                    Corner::BottomLeft.label(),
                    self.corner == Corner::BottomLeft,
                    Message::Pick(Corner::BottomLeft),
                ),
                chip(
                    Corner::BottomRight.label(),
                    self.corner == Corner::BottomRight,
                    Message::Pick(Corner::BottomRight),
                ),
            ]
            .spacing(8),
            button(text(show).size(16).color(BG))
                .on_press(Message::Arm)
                .width(Length::Fill)
                .style(|_, status| button::Style {
                    background: Some(Background::Color(match status {
                        button::Status::Hovered => Color::from_rgb8(0x89, 0xb4, 0xfa),
                        button::Status::Pressed => Color::from_rgb8(0x5d, 0x8a, 0xe0),
                        _ => ACCENT,
                    })),
                    text_color: BG,
                    border: Border {
                        radius: 12.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }),
            text(status).size(14).color(FG),
            button(text("Quit").size(14).color(MUTED))
                .on_press(Message::Quit)
                .style(|_, _| button::Style {
                    background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.0))),
                    text_color: MUTED,
                    border: Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: LINE,
                    },
                    ..button::Style::default()
                }),
        ]
        .spacing(12)
        .into()
    }

    fn popup(&self) -> Element<'_, Message> {
        let place = self.corner.label().to_lowercase();
        let footer = match self.answer {
            Some(label) => text(format!("Path locked  ·  {label}"))
                .size(13)
                .color(GOLD_HOT),
            None => text("Two paths. Pick one.").size(13).color(MUTED),
        };

        column![
            row![
                text("QUEST").size(12).color(GOLD),
                text("01").size(12).color(MUTED),
                iced::widget::space::horizontal(),
                button(text("×").size(16).color(GOLD))
                    .on_press(Message::ClosePopup)
                    .padding([4, 10])
                    .style(|_, status| button::Style {
                        background: Some(Background::Color(Color::from_rgba8(
                            0xe8,
                            0xc3,
                            0x72,
                            if status == button::Status::Hovered {
                                0.18
                            } else {
                                0.0
                            },
                        ))),
                        text_color: GOLD,
                        border: Border {
                            radius: 999.0.into(),
                            width: 1.0,
                            color: GOLD,
                        },
                        ..button::Style::default()
                    }),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            container(iced::widget::space::horizontal())
                .width(Length::Fill)
                .height(Length::Fixed(2.0))
                .style(|_| container::Style {
                    background: Some(Background::Color(GOLD)),
                    ..Default::default()
                }),
            text("A card slipped in.").size(26).color(FG),
            text(format!("You called it to the {place}."))
                .size(14)
                .color(MUTED),
            choice(
                "A",
                "Yes. This corner.",
                "Hold this position",
                "right corner",
                self.answer == Some("right corner"),
            ),
            choice(
                "B",
                "No. Wrong spot.",
                "It landed somewhere else",
                "wrong place",
                self.answer == Some("wrong place"),
            ),
            footer,
        ]
        .spacing(12)
        .into()
    }
}

fn card(inner: Element<'_, Message>) -> Element<'_, Message> {
    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(22)
        .style(|_| container::Style {
            background: Some(Background::Color(CARD)),
            text_color: Some(FG),
            border: Border {
                color: LINE,
                width: 1.0,
                radius: 18.0.into(),
            },
            shadow: Shadow {
                color: Color {
                    a: 0.45,
                    ..Color::BLACK
                },
                offset: Vector::new(0.0, 12.0),
                blur_radius: 28.0,
            },
            ..Default::default()
        })
        .into()
}

fn chip(label: &str, on: bool, message: Message) -> Element<'_, Message> {
    button(text(label).size(14))
        .on_press(message)
        .style(move |_, status| {
            let fill = if on {
                ACCENT
            } else if status == button::Status::Hovered {
                Color::from_rgb8(0x2f, 0x35, 0x4d)
            } else {
                BG
            };
            button::Style {
                background: Some(Background::Color(fill)),
                text_color: if on { BG } else { FG },
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: if on { ACCENT } else { LINE },
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn quest_card(inner: Element<'_, Message>) -> Element<'_, Message> {
    let fill = Linear::new(Radians(1.15))
        .add_stop(0.0, Color::from_rgb8(0x3a, 0x22, 0x58))
        .add_stop(0.45, Color::from_rgb8(0x1c, 0x16, 0x32))
        .add_stop(1.0, Color::from_rgb8(0x10, 0x12, 0x1e));

    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(22)
        .style(move |_| container::Style {
            background: Some(Background::from(fill)),
            text_color: Some(FG),
            border: Border {
                color: GOLD,
                width: 1.5,
                radius: 22.0.into(),
            },
            shadow: Shadow {
                color: Color {
                    r: 0.91,
                    g: 0.76,
                    b: 0.45,
                    a: 0.28,
                },
                offset: Vector::new(0.0, 14.0),
                blur_radius: 32.0,
            },
            ..Default::default()
        })
        .into()
}

fn choice(
    letter: &'static str,
    title: &'static str,
    detail: &'static str,
    recorded: &'static str,
    on: bool,
) -> Element<'static, Message> {
    let mark = container(text(letter).size(15).color(if on { INK } else { GOLD }))
        .width(34)
        .height(34)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(if on {
                GOLD_HOT
            } else {
                Color::from_rgba8(0xe8, 0xc3, 0x72, 0.12)
            })),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: if on { GOLD_HOT } else { GOLD },
            },
            ..Default::default()
        });

    let body = column![
        text(title)
            .size(16)
            .color(if on { INK } else { FG }),
        text(detail)
            .size(12)
            .color(if on {
                Color::from_rgb8(0x5c, 0x43, 0x16)
            } else {
                MUTED
            }),
    ]
    .spacing(2);

    button(row![mark, body].spacing(12).align_y(Alignment::Center))
        .on_press(Message::Answer(recorded))
        .width(Length::Fill)
        .padding(10)
        .style(move |_, status| {
            let hot = on || status == button::Status::Hovered;
            button::Style {
                background: Some(Background::Color(if on {
                    GOLD
                } else if hot {
                    Color::from_rgba8(0xe8, 0xc3, 0x72, 0.14)
                } else {
                    Color::from_rgba8(0, 0, 0, 0.28)
                })),
                text_color: if on { INK } else { FG },
                border: Border {
                    radius: 14.0.into(),
                    width: if on { 1.5 } else { 1.0 },
                    color: if hot { GOLD_HOT } else { Color::from_rgba8(0xe8, 0xc3, 0x72, 0.45) },
                },
                shadow: Shadow {
                    color: Color {
                        r: 0.91,
                        g: 0.76,
                        b: 0.45,
                        a: if on { 0.35 } else { 0.0 },
                    },
                    offset: Vector::new(0.0, 0.0),
                    blur_radius: 16.0,
                },
                ..button::Style::default()
            }
        })
        .into()
}

#[cfg(test)]
mod popup_check;

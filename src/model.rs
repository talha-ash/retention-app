/// Identity of one note. Empty strings are rejected at creation.
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Deck name, including any `::` separators Anki stores in the name.
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::fmt::Display for DeckName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Id of one option inside a Choice note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceId(String);

impl ChoiceId {
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

impl std::fmt::Display for ChoiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub id: ChoiceId,
    pub text: String,
}

/// What a note studies. Card counts are fixed by the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteBody {
    Basic {
        front: String,
        back: String,
    },
    Reversed {
        front: String,
        back: String,
    },
    Cloze {
        text: String,
    },
    Choice {
        prompt: String,
        choices: Vec<Choice>,
        correct: Vec<ChoiceId>,
        explain: String,
    },
}

impl NoteBody {
    pub fn study_cards(&self) -> u32 {
        match self {
            NoteBody::Reversed { .. } => 2,
            NoteBody::Basic { .. } | NoteBody::Cloze { .. } | NoteBody::Choice { .. } => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub guid: Guid,
    pub deck: DeckName,
    pub body: NoteBody,
}

/// Notes read from one file. `scheduling_rows` is a count of card rows seen in an apkg.
/// Text import sets it to 0. The count is never applied to memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub notes: Vec<Note>,
    pub skipped: u32,
    pub scheduling_rows: u32,
}

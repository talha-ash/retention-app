//! A small Markdown subset for a card face.
//!
//! Headings, paragraphs, and list items. Anything else stays a paragraph.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph(String),
    Item(String),
}

pub fn blocks(source: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let flush = |paragraph: &mut Vec<&str>, out: &mut Vec<Block>| {
        if paragraph.is_empty() {
            return;
        }
        out.push(Block::Paragraph(paragraph.join(" ")));
        paragraph.clear();
    };
    for raw in source.split('\n') {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            flush(&mut paragraph, &mut out);
            continue;
        }
        if let Some((level, text)) = heading(line) {
            flush(&mut paragraph, &mut out);
            if !text.is_empty() {
                out.push(Block::Heading {
                    level,
                    text: text.to_string(),
                });
            }
            continue;
        }
        if let Some(text) = item(line) {
            flush(&mut paragraph, &mut out);
            if !text.is_empty() {
                out.push(Block::Item(text.to_string()));
            }
            continue;
        }
        paragraph.push(line);
    }
    flush(&mut paragraph, &mut out);
    out
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=3).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((hashes as u8, rest.trim()))
}

fn item(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))?;
    Some(rest.trim())
}

#[cfg(test)]
mod tests {
    use super::{Block, blocks};

    #[test]
    fn plain_line_is_one_paragraph() {
        assert_eq!(blocks("to speak"), vec![Block::Paragraph("to speak".into())]);
    }

    #[test]
    fn headings_paragraphs_and_items() {
        let source = "# Vowels\n\na and e\nstay together\n\n- open\n- closed\n## Stress";
        assert_eq!(
            blocks(source),
            vec![
                Block::Heading {
                    level: 1,
                    text: "Vowels".into(),
                },
                Block::Paragraph("a and e stay together".into()),
                Block::Item("open".into()),
                Block::Item("closed".into()),
                Block::Heading {
                    level: 2,
                    text: "Stress".into(),
                },
            ]
        );
    }

    #[test]
    fn stars_without_a_space_stay_text() {
        assert_eq!(
            blocks("**bold**"),
            vec![Block::Paragraph("**bold**".into())]
        );
    }
}

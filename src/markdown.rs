//! A small Markdown subset for a card face.
//!
//! Headings, paragraphs, list items, bold, and italics. Anything else stays text.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph(String),
    Item(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    Plain(String),
    Strong(String),
    Em(String),
}

pub fn marks(source: &str) -> Vec<Mark> {
    let mut out = Vec::new();
    for mark in split_pair(source, "**", Mark::Strong) {
        match mark {
            Mark::Plain(text) => out.extend(split_pair(&text, "*", Mark::Em)),
            other => out.push(other),
        }
    }
    out
}

fn split_pair(source: &str, marker: &str, wrap: fn(String) -> Mark) -> Vec<Mark> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(start) = find_marker(rest, marker) {
        let after = start + marker.len();
        let Some(end) = find_marker(&rest[after..], marker) else {
            break;
        };
        push_plain(&mut out, &rest[..start]);
        let inner = &rest[after..after + end];
        if !inner.is_empty() {
            out.push(wrap(inner.to_string()));
        }
        rest = &rest[after + end + marker.len()..];
    }
    push_plain(&mut out, rest);
    out
}

fn find_marker(source: &str, marker: &str) -> Option<usize> {
    if marker != "*" {
        return source.find(marker);
    }
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let alone = bytes[index] == b'*'
            && (index == 0 || bytes[index - 1] != b'*')
            && (index + 1 >= bytes.len() || bytes[index + 1] != b'*');
        if alone {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn push_plain(out: &mut Vec<Mark>, text: &str) {
    if !text.is_empty() {
        out.push(Mark::Plain(text.to_string()));
    }
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
    use super::{Block, Mark, blocks, marks};

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

    #[test]
    fn bold_and_italics_split_the_line() {
        assert_eq!(
            marks("say **bold** and *slant* now"),
            vec![
                Mark::Plain("say ".into()),
                Mark::Strong("bold".into()),
                Mark::Plain(" and ".into()),
                Mark::Em("slant".into()),
                Mark::Plain(" now".into()),
            ]
        );
    }

    #[test]
    fn an_unclosed_marker_stays_text() {
        assert_eq!(marks("**bold"), vec![Mark::Plain("**bold".into())]);
    }
}

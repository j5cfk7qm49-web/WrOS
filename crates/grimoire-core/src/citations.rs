//! Source ranges and the project citation marker grammar.

use crate::sources::valid_key;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub mod mla;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cite<'a> {
    pub key: &'a str,
    pub page: Option<&'a str>,
    pub suppress_author: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker<'a> {
    pub range: Range<usize>,
    pub items: Result<Vec<Cite<'a>>, &'static str>,
}

/// Return markers in original source order. Escaped brackets and ranges
/// owned by links, images, HTML and code are skipped.
pub fn markers(text: &str) -> Vec<Marker<'_>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut excluded = 0usize;
    for (event, range) in Parser::new_ext(text, Options::ENABLE_FOOTNOTES).into_offset_iter() {
        match event {
            Event::Start(
                Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_) | Tag::HtmlBlock,
            ) => {
                excluded += 1;
            }
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock | TagEnd::HtmlBlock) => {
                excluded -= 1;
            }
            Event::Text(_) if excluded == 0 => {
                if let Some(previous) = ranges.last_mut().filter(|prev| prev.end == range.start) {
                    previous.end = range.end;
                } else {
                    ranges.push(range);
                }
            }
            _ => (),
        }
    }
    let mut found = Vec::new();
    for range in ranges {
        let mut search = range.start;
        while let Some(pos) = text[search..range.end].find('[') {
            let start = search + pos;
            search = start + 1;
            let remaining = &text[search..range.end];
            if !(remaining.starts_with('@') || remaining.starts_with("-@")) || escaped(text, start)
            {
                continue;
            }
            let end = remaining
                .find([']', '\n', '['])
                .map_or(range.end, |offset| search + offset);
            let closed = text.as_bytes().get(end) == Some(&b']') && end < range.end;
            let items = if closed {
                parse(&text[search..end])
            } else {
                Err("Close the citation with ]")
            };
            search = end + usize::from(closed);
            found.push(Marker {
                range: start..search,
                items,
            });
        }
    }
    found
}

fn escaped(text: &str, index: usize) -> bool {
    text[..index]
        .bytes()
        .rev()
        .take_while(|&b| b == b'\\')
        .count()
        % 2
        == 1
}

fn parse(source: &str) -> Result<Vec<Cite<'_>>, &'static str> {
    source
        .split(';')
        .map(|item| {
            let item = item.trim();
            let (item, suppress_author) = item
                .strip_prefix('-')
                .map_or((item, false), |rest| (rest, true));
            let item = item
                .strip_prefix('@')
                .ok_or("Start each citation with @ or -@")?;
            let (key, page) = match item.split_once(',') {
                Some((key, locator)) => {
                    let locator = locator.trim();
                    let page = locator
                        .strip_prefix("p.")
                        .or_else(|| locator.strip_prefix("pp."))
                        .ok_or("Use p. or pp. before a page locator")?
                        .trim();
                    if !valid_page(page) {
                        return Err("Enter a page, a range or a list of pages");
                    }
                    (key.trim(), Some(page))
                }
                None => (item, None),
            };
            if !valid_key(key) {
                return Err("Use a project source key after @");
            }
            Ok(Cite {
                key,
                page,
                suppress_author,
            })
        })
        .collect()
}

/// A page (194), a range (12–14), or a list of them for pages that aren't consecutive (54, 56),
/// as MLA 9 writes them.
pub fn valid_page(page: &str) -> bool {
    !page.is_empty()
        && page.len() <= 64
        && page.split(',').all(|item| {
            let item = item.trim();
            !item.is_empty()
                && item.split(['-', '–', '—']).all(|part| {
                    let part = part.trim();
                    !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric())
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_can_be_a_list_of_pages_and_ranges() {
        for good in [
            "194",
            "12–14",
            "54, 56",
            "54,56",
            "12–14, 20",
            "xii, 3",
            "1-2, 4-5, 9",
        ] {
            assert!(valid_page(good), "{good}");
        }
        for bad in [
            "",
            "54,",
            ",56",
            "54,,56",
            "54; 56",
            "54, 56–",
            "54 56",
            "p. 54",
        ] {
            assert!(!valid_page(bad), "{bad}");
        }
        let marks = markers("Café [@freud, pp. 54, 56] and [@freud, p. 12].");
        assert_eq!(marks[0].items.as_ref().unwrap()[0].page, Some("54, 56"));
        assert_eq!(marks[1].items.as_ref().unwrap()[0].page, Some("12"));
    }

    #[test]
    fn recognizes_clusters_and_keeps_original_source_ranges() {
        let source = "Café [@one, pp. 12–14; -@two, p. 5].";
        let marks = markers(source);
        assert_eq!(marks.len(), 1);
        assert_eq!(
            &source[marks[0].range.clone()],
            "[@one, pp. 12–14; -@two, p. 5]"
        );
        assert_eq!(marks[0].items.as_ref().unwrap()[0].page, Some("12–14"));
        assert!(marks[0].items.as_ref().unwrap()[1].suppress_author);
    }

    #[test]
    fn respects_markdown_context_and_reports_broken_syntax() {
        let source = r#"name@somewhere.example [@label](/url) ![@picture](/image)
\[@escaped] `[@code]`

~~~md
[@fence]
~~~

[@bad, time 1] and [@ok] and [-@missing
"#;
        let marks = markers(source);
        assert_eq!(marks.len(), 3, "{marks:?}");
        assert!(marks[0].items.is_err());
        assert_eq!(marks[1].items.as_ref().unwrap()[0].key, "ok");
        assert!(marks[2].items.is_err());
    }
}

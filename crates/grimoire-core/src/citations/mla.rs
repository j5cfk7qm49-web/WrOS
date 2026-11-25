//! The pinned MLA 9 citation style, shared by the Word and PDF renderers.

use anyhow::{Context, Result, ensure};
use hayagriva::citationberg::{IndependentStyle, LocaleFile, json::Item, taxonomy::Locator};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest, ElemChild, ElemMeta,
    Formatted, Formatting, LocatorPayload, SpecificLocator,
};
use std::collections::BTreeMap;

use super::Cite;
use crate::sources::{Record, Sources};

const STYLE: &str = include_str!("../../../../assets/csl/modern-language-association.csl");
const LOCALE: &str = include_str!("../../../../assets/csl/locales-en-US.xml");

#[derive(Debug)]
pub struct PaperCitations {
    pub citations: Vec<Vec<Formatted>>,
    pub works_cited: Vec<Vec<Formatted>>,
}

pub fn works_cited_line(record: &Record) -> Result<String> {
    let key = "preview";
    let mut record = record.clone();
    record.insert("id".into(), serde_json::Value::String(key.into()));
    let mut sources = Sources::default();
    sources.add(record)?;
    let rendered = render(
        &sources,
        &[vec![Cite {
            key,
            page: None,
            suppress_author: false,
        }]],
    )?;
    let entry = rendered
        .works_cited
        .first()
        .context("The MLA processor returned no Works Cited entry.")?;
    Ok(entry.iter().map(|run| run.text.as_str()).collect())
}

pub fn render(sources: &Sources, groups: &[Vec<Cite<'_>>]) -> Result<PaperCitations> {
    let style = IndependentStyle::from_xml(STYLE).context("Reading the bundled MLA 9 style")?;
    let locales = [LocaleFile::from_xml(LOCALE)
        .context("Reading the bundled MLA locale")?
        .into()];

    let mut records = BTreeMap::new();
    for cite in groups.iter().flatten() {
        if records.contains_key(cite.key) {
            continue;
        }
        let mut data = sources
            .get(cite.key)
            .with_context(|| format!("No project source uses the key {}.", cite.key))?
            .clone();
        data.remove("_wros");
        let item: Item = serde_json::from_value(serde_json::Value::Object(data))
            .with_context(|| format!("Check the details for source {}.", cite.key))?;
        records.insert(cite.key, item);
    }

    let mut driver = BibliographyDriver::new();
    for group in groups {
        driver.citation(CitationRequest::from_items(
            group
                .iter()
                .map(|cite| {
                    CitationItem::with_locator(
                        &records[cite.key],
                        cite.page
                            .map(|page| SpecificLocator(Locator::Page, LocatorPayload::Str(page))),
                    )
                })
                .collect(),
            &style,
            &locales,
        ));
    }
    let rendered = driver.finish(BibliographyRequest::new(&style, None, &locales));
    ensure!(
        rendered.citations.len() == groups.len(),
        "The MLA processor returned an incomplete citation list."
    );
    let citations = rendered
        .citations
        .into_iter()
        .zip(groups)
        .map(|(cite, group)| {
            if group.iter().any(|c| c.suppress_author) {
                suppress_names(&cite.citation.0, group)
            } else {
                let mut out = Vec::new();
                flatten(&cite.citation.0, false, &mut out)?;
                Ok(out)
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let mut works_cited = Vec::new();
    if let Some(bibliography) = rendered.bibliography {
        for entry in bibliography.items {
            let mut runs = Vec::new();
            flatten(&entry.content.0, false, &mut runs)?;
            works_cited.push(runs);
        }
    }
    Ok(PaperCitations {
        citations,
        works_cited,
    })
}

fn suppress_names(children: &[ElemChild], group: &[Cite<'_>]) -> Result<Vec<Formatted>> {
    // The MLA layout wraps each source in an Entry node. Keep the locator and
    // disambiguating title from the regular citation, and remove only names.
    let entries: Vec<_> = children
        .iter()
        .filter_map(|child| match child {
            ElemChild::Elem(elem) => match elem.meta {
                Some(ElemMeta::Entry(i)) => Some((i, &elem.children.0)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    ensure!(
        entries.len() == group.len(),
        "The MLA processor could not distinguish the sources in this citation."
    );
    let mut out = Vec::new();
    for (i, children) in entries {
        let cite = group
            .get(i)
            .context("The MLA processor returned an unknown source")?;
        let mut entry = Vec::new();
        flatten(children, cite.suppress_author, &mut entry)?;
        while entry.first().is_some_and(|t| t.text.trim().is_empty()) {
            entry.remove(0);
        }
        if let Some(first) = entry.first_mut() {
            first.text = first.text.trim_start().to_owned();
        }
        if entry.is_empty() {
            continue;
        }
        out.push(plain(if out.is_empty() { "(" } else { "; " }));
        out.extend(entry);
    }
    if !out.is_empty() {
        out.push(plain(")"));
    }
    Ok(out)
}

fn plain(text: &str) -> Formatted {
    Formatted {
        text: text.to_owned(),
        formatting: Formatting::default(),
    }
}

fn flatten(children: &[ElemChild], omit_names: bool, runs: &mut Vec<Formatted>) -> Result<()> {
    for child in children {
        match child {
            ElemChild::Text(run) | ElemChild::Link { text: run, .. } => runs.push(run.clone()),
            ElemChild::Elem(e)
                if omit_names && matches!(e.meta, Some(ElemMeta::Names | ElemMeta::Name(..))) => {}
            ElemChild::Elem(e) => flatten(&e.children.0, omit_names, runs)?,
            ElemChild::Markup(_) | ElemChild::Transparent { .. } => {
                anyhow::bail!("The MLA processor returned unsupported output.")
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baron_mla_fixtures_produce_pages_and_a_styled_works_cited() {
        let sources = Sources::parse(r#"[{"id":"baron","type":"article-journal","title":"Redefining Reading: The Impact of Digital Communication Media","author":[{"family":"Baron","given":"Naomi S."}],"container-title":"PMLA","volume":"128","issue":"1","issued":{"date-parts":[[2013,1]]},"page":"193-200"}]"#).unwrap();
        let groups = vec![
            vec![Cite {
                key: "baron",
                page: Some("194"),
                suppress_author: false,
            }],
            vec![Cite {
                key: "baron",
                page: Some("194"),
                suppress_author: true,
            }],
            vec![Cite {
                key: "baron",
                page: None,
                suppress_author: true,
            }],
        ];
        let result = render(&sources, &groups).unwrap();
        let plain = |runs: &[Formatted]| runs.iter().map(|r| r.text.as_str()).collect::<String>();
        assert_eq!(plain(&result.citations[0]), "(Baron 194)");
        assert_eq!(plain(&result.citations[1]), "(194)");
        assert_eq!(plain(&result.citations[2]), "");
        assert_eq!(result.works_cited.len(), 1);
        assert!(
            plain(&result.works_cited[0]).ends_with("vol. 128, no. 1, Jan. 2013, pp. 193–200.")
        );
        assert!(result.works_cited[0].iter().any(|r| r.text == "PMLA"
            && r.formatting.font_style == hayagriva::citationberg::FontStyle::Italic));
    }
}

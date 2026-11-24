use std::collections::BTreeMap;

use grimoire_brain::bounded_store::PagedBrain;
use grimoire_brain::model::{Builtin, FieldId, NodeId};
use grimoire_brain::schema::{FieldKind, FieldSpec, SavedQuery};

#[derive(Clone, Copy)]
pub(super) enum Command {
    Find,
    SaveSearch(NodeId),
    Link(NodeId),
    Tag(NodeId),
    Field(NodeId),
    Schema,
}

impl Command {
    pub(super) fn hint(self) -> &'static str {
        match self {
            Self::Find | Self::SaveSearch(_) => {
                "Search phrase, #tag, @title, or attached field=value"
            }
            Self::Link(_) => "Link to a note by title or ID · use -title to remove",
            Self::Tag(_) => "Add a tag by name or page title · use -tag to remove",
            Self::Field(_) => "Field name = value · leave the value empty to clear",
            Self::Schema => "Tag / field = type · rename with old name -> new name",
        }
    }

    pub(super) fn apply(
        self,
        store: &mut PagedBrain,
        input: &str,
        selected: Option<NodeId>,
    ) -> anyhow::Result<Option<SavedQuery>> {
        let input = input.trim();
        match self {
            Self::Find => Ok(Some(parse_query(store, input, selected)?)),
            Self::SaveSearch(node) => {
                let query = if input.is_empty() {
                    None
                } else {
                    Some(parse_query(store, input, Some(node))?)
                };
                store
                    .set_search(node, query.clone())
                    .map_err(anyhow::Error::msg)?;
                Ok(query)
            }
            Self::Link(node) | Self::Tag(node) => {
                let (target, present) = input
                    .strip_prefix('-')
                    .map_or((input, true), |name| (name.trim(), false));
                if matches!(self, Self::Tag(_)) {
                    let id = find_tag(store, target)?;
                    store
                        .set_tag(node, id, present)
                        .map_err(anyhow::Error::msg)?;
                } else {
                    let id = find_note(store, target)?;
                    store
                        .set_link(node, id, present)
                        .map_err(anyhow::Error::msg)?;
                }
                Ok(None)
            }
            Self::Field(node) => {
                let (name, value) = input
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("Enter a field name and ="))?;
                let field = field_named(&fields_for_note(store, node)?, name)?;
                let value = value.trim();
                store
                    .set_field(node, field, (!value.is_empty()).then(|| value.into()))
                    .map_err(anyhow::Error::msg)?;
                Ok(None)
            }
            Self::Schema => {
                let (tag, change) = input
                    .split_once('/')
                    .ok_or_else(|| anyhow::anyhow!("Enter a tag, /, and a field name"))?;
                let tag = find_tag(store, tag.trim())?;
                let (field, kind) = change
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("Enter a field name and a type"))?;
                let old = store.fields_for_tag(tag).map_err(anyhow::Error::msg)?;
                let (old_name, name) = field
                    .split_once("->")
                    .map_or((field, field), |(old, name)| (old, name));
                let field = match field_named(&old, old_name) {
                    Ok(id) => Some(id),
                    Err(error) if old_name != name => return Err(error),
                    Err(error)
                        if old
                            .values()
                            .any(|spec| spec.name.eq_ignore_ascii_case(old_name.trim())) =>
                    {
                        return Err(error);
                    }
                    Err(_) => None,
                };
                let kind = kind.trim();
                let (kind, required) = kind
                    .strip_suffix('!')
                    .map_or((kind, false), |kind| (kind.trim(), true));
                let kind = match kind {
                    "text" => FieldKind::Text,
                    "number" => FieldKind::Number,
                    "date" => FieldKind::Date,
                    "link" => FieldKind::Link,
                    "url" => FieldKind::Url,
                    "file" => FieldKind::File,
                    _ if kind.starts_with("choice(") && kind.ends_with(')') => {
                        let choices: Vec<_> = kind[7..kind.len() - 1]
                            .split('|')
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned)
                            .collect();
                        anyhow::ensure!(!choices.is_empty(), "Enter choices separated by |");
                        FieldKind::Choice(choices)
                    }
                    _ => anyhow::bail!("Use text, number, date, link, url, file, or choice(a|b)"),
                };
                store
                    .define_field(
                        tag,
                        field,
                        FieldSpec {
                            name: name.trim().into(),
                            kind,
                            required,
                        },
                    )
                    .map_err(anyhow::Error::msg)?;
                Ok(None)
            }
        }
    }
}

pub(super) fn find_tag(store: &mut PagedBrain, text: &str) -> anyhow::Result<NodeId> {
    let text = text.trim().trim_start_matches('#');
    if let Some(tag) = Builtin::ALL
        .into_iter()
        .find(|tag| tag.name().eq_ignore_ascii_case(text))
    {
        return Ok(NodeId::Builtin(tag));
    }
    find_note(store, text)
}

fn find_note(store: &mut PagedBrain, text: &str) -> anyhow::Result<NodeId> {
    let text = text.trim();
    if let Ok(id) = text.parse() {
        return Ok(id);
    }
    let rows = store
        .search(&SavedQuery::Text(text.into()), 0, 120)
        .map_err(anyhow::Error::msg)?;
    let exact: Vec<_> = rows
        .iter()
        .filter(|row| super::note_title(&row.body).eq_ignore_ascii_case(text))
        .collect();
    match (exact.as_slice(), rows.as_slice()) {
        ([row], _) => Ok(row.id),
        ([], [row]) => Ok(row.id),
        (_, []) => anyhow::bail!("No Brain notes match"),
        _ => anyhow::bail!("More than one note matches. Enter an exact title or ID"),
    }
}

fn parse_query(
    store: &mut PagedBrain,
    input: &str,
    selected: Option<NodeId>,
) -> anyhow::Result<SavedQuery> {
    anyhow::ensure!(!input.trim().is_empty(), "Enter something to find");
    if let Some(tag) = input.strip_prefix('#') {
        return Ok(SavedQuery::Tagged(find_tag(store, tag)?));
    }
    if let Some(target) = input.strip_prefix('@') {
        return Ok(SavedQuery::LinksTo(find_note(store, target)?));
    }
    if let Some((name, value)) = input.split_once('=') {
        let node = selected.ok_or_else(|| anyhow::anyhow!("Select a note with the field first"))?;
        let field = field_named(&fields_for_note(store, node)?, name)?;
        return Ok(SavedQuery::Field {
            field,
            value: value.trim().into(),
        });
    }
    Ok(SavedQuery::Text(input.into()))
}

fn fields_for_note(
    store: &mut PagedBrain,
    node: NodeId,
) -> anyhow::Result<BTreeMap<FieldId, FieldSpec>> {
    let details = store.details(node).map_err(anyhow::Error::msg)?;
    let mut fields = BTreeMap::new();
    for tag in details.tags {
        fields.extend(store.fields_for_tag(tag).map_err(anyhow::Error::msg)?);
    }
    Ok(fields)
}

fn field_named(fields: &BTreeMap<FieldId, FieldSpec>, name: &str) -> anyhow::Result<FieldId> {
    let mut matching = fields
        .iter()
        .filter(|(_, spec)| spec.name.eq_ignore_ascii_case(name.trim()));
    let (&id, _) = matching
        .next()
        .ok_or_else(|| anyhow::anyhow!("No field has that name"))?;
    anyhow::ensure!(
        matching.next().is_none(),
        "More than one field has that name"
    );
    Ok(id)
}

pub(super) fn describe(store: &mut PagedBrain, node: NodeId) -> anyhow::Result<Vec<String>> {
    let details = store.details(node).map_err(anyhow::Error::msg)?;
    let mut lines = vec![format!("Note ID  {}", node.external_id())];
    let mut schema = BTreeMap::new();
    for tag in details.tags {
        let label = label(store, tag)?;
        lines.push(format!("#{label}"));
        schema.extend(store.fields_for_tag(tag).map_err(anyhow::Error::msg)?);
    }
    for (&id, spec) in &schema {
        let value = details
            .fields
            .iter()
            .find_map(|(field, value)| (*field == id).then_some(value.as_str()));
        let hint = match value {
            Some(value) if !spec.kind.accepts(value) => " · needs conversion",
            None if spec.required => " · required",
            _ => "",
        };
        lines.push(format!("{}  {}{hint}", spec.name, value.unwrap_or("")));
    }
    for (id, value) in details.fields {
        if !schema.contains_key(&id) {
            lines.push(format!("Retained field  {value}"));
        }
    }
    for link in details.links {
        lines.push(format!("→ {}", label(store, link)?));
    }
    for row in store
        .search(&SavedQuery::LinksTo(node), 0, 16)
        .map_err(anyhow::Error::msg)?
    {
        lines.push(format!("← {}", super::note_title(&row.body)));
    }
    if details.query.is_some() {
        lines.push("Saved search · R opens results".into());
    }
    Ok(lines)
}

pub(super) fn label(store: &mut PagedBrain, node: NodeId) -> anyhow::Result<String> {
    match node {
        NodeId::Builtin(tag) => Ok(tag.name().to_owned()),
        NodeId::Today(date) => Ok(date.to_string()),
        _ => Ok(store
            .note(node)
            .map_err(anyhow::Error::msg)?
            .map(|text| super::note_title(&text).to_owned())
            .unwrap_or_else(|| node.external_id())),
    }
}

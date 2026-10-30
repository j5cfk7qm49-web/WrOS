//! Shared persistence checks for the experimental story models. Reuses the
//! existing bounded, conflict-aware metadata store; these are not editor UIs.
use crate::statefile::{Store, Validate};
use anyhow::{Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, path::Path};

pub(crate) fn load<T: Validate + Serialize + DeserializeOwned + Clone>(
    root: &Path,
    stem: &str,
    default: T,
) -> Result<Store<T>> {
    match std::fs::symlink_metadata(root.join(format!(".grimoire/{stem}.json"))) {
        Ok(_) => {
            anyhow::bail!("legacy story metadata exists; original preserved; migration required")
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Store::load(
        root,
        Path::new(&format!(".grimoire/writingdeck/{stem}.json")),
        default,
    )
}

pub(crate) fn identities(schema: u32, next: u64, ids: impl IntoIterator<Item = u64>) -> Result<()> {
    ensure!(schema == 1, "unknown story schema; original preserved");
    let mut seen = BTreeSet::new();
    for id in ids {
        ensure!(
            seen.len() < 4096,
            "story record limit reached; original preserved"
        );
        ensure!(
            id > 0 && id <= next && seen.insert(id),
            "invalid or duplicate story identity"
        );
    }
    Ok(())
}
pub(crate) fn text(value: &str, max: usize) -> Result<()> {
    ensure!(
        value.len() <= max,
        "story field exceeds limit; original preserved"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        annotations::AnnotationStore,
        continuity::ContinuityStore,
        milestones::{MilestoneStore, VariantStore},
    };
    fn preservation<T: Validate + Serialize + DeserializeOwned + Clone + Default>(
        stem: &str,
        open: impl Fn(&Path) -> Result<Store<T>>,
    ) {
        let root = std::env::temp_dir().join(format!("wros-story-{stem}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut store = open(&root).unwrap();
        assert!(!store.exists());
        store.update(|_| Ok(())).unwrap();
        let path = root.join(format!(".grimoire/writingdeck/{stem}.json"));
        let mut future = serde_json::to_value(T::default()).unwrap();
        future["schema_version"] = (future["schema_version"].as_u64().unwrap() + 1).into();
        let mut unknown = serde_json::to_value(T::default()).unwrap();
        unknown["newer_field"] = true.into();
        for bytes in [
            b"invalid JSON".to_vec(),
            serde_json::to_vec(&future).unwrap(),
            serde_json::to_vec(&unknown).unwrap(),
            vec![b' '; crate::statefile::MAX_BYTES + 1],
        ] {
            std::fs::write(&path, &bytes).unwrap();
            assert!(open(&root).is_err());
            assert!(store.update(|_| Ok(())).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        std::fs::remove_file(&path).unwrap();
        if T::default().validate().is_ok()
            && serde_json::to_value(T::default()).unwrap()["schema_version"] == 2
        {
            let mut old = serde_json::to_value(T::default()).unwrap();
            old["schema_version"] = 1.into();
            let bytes = serde_json::to_vec(&old).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            assert!(
                open(&root).is_err(),
                "index-based prototypes must not become durable document IDs"
            );
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            std::fs::remove_file(&path).unwrap();
        }
        let legacy = root.join(format!(".grimoire/{stem}.json"));
        std::fs::write(&legacy, b"original unversioned prototype data").unwrap();
        assert!(open(&root).is_err());
        assert_eq!(
            std::fs::read(&legacy).unwrap(),
            b"original unversioned prototype data"
        );
        assert!(!path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn all_story_stores_preserve_corrupt_future_external_oversize_and_legacy_data() {
        preservation::<AnnotationStore>("annotations", AnnotationStore::load);
        preservation::<ContinuityStore>("continuity", ContinuityStore::load);
        preservation::<MilestoneStore>("milestones", MilestoneStore::load);
        preservation::<VariantStore>("variants", VariantStore::load);
    }
}

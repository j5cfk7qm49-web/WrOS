//! The readable Google Drive sync helper (`wros-sync`), run like the backup
//! helper: its own session, no terminal, cancellable, progress on stderr.
use crate::backup_service::{Job, run_helper, spawn_work};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum Request {
    Enable {
        project: String,
    },
    Disable {
        project: String,
    },
    Sync {
        project: String,
        allow_deletes: bool,
    },
}

/// A file changed on both sides, with Drive's version parked beside it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Pending {
    pub path: String,
    pub parked: String,
    /// The last version both sides shared, kept privately for the merge.
    pub merge_base: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Reply {
    /// enabled, disabled, synced, confirm, offline or refused.
    pub status: String,
    pub message: String,
    pub uploaded: u32,
    pub downloaded: u32,
    pub deleted: u32,
    pub deletes: u32,
    pub pending: Vec<Pending>,
    pub synced_at: Option<String>,
    pub drive_folder: Option<String>,
}

#[derive(Deserialize)]
struct Response {
    ok: bool,
    data: Option<Reply>,
    error: Option<String>,
}

const UNAVAILABLE: &str = "Sync unavailable; install the WrOS Google Drive integration.";

/// The installed helper, or the repository's in a debug build.
pub fn script() -> Option<PathBuf> {
    let installed = Path::new("/usr/libexec/wros-sync");
    if installed.is_file() {
        Some(installed.to_path_buf())
    } else if cfg!(debug_assertions) {
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("os/scripts/wros-sync"))
    } else {
        None
    }
}

pub fn spawn(
    request: Request,
    script: PathBuf,
    env: Vec<(&'static str, PathBuf)>,
) -> Result<Job<Reply>, String> {
    spawn_work("wros-sync", move |cancelled, progress| {
        let input = serde_json::to_vec(&request).map_err(|_| UNAVAILABLE.to_owned())?;
        let bytes = run_helper(
            &script,
            &input,
            &env,
            cancelled,
            Some(progress),
            UNAVAILABLE,
            "Sync stopped; files not yet done are unchanged and sync again next time.",
        )?;
        let response: Response =
            serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE.to_owned())?;
        if response.ok {
            response.data.ok_or_else(|| UNAVAILABLE.to_owned())
        } else {
            Err(response.error.unwrap_or_else(|| UNAVAILABLE.to_owned()))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_name_their_operation_and_replies_tolerate_missing_counts() {
        let request = Request::Sync {
            project: "/book".into(),
            allow_deletes: true,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"operation":"sync","project":"/book","allow_deletes":true}"#
        );
        let reply: Reply = serde_json::from_str(
            r#"{"status":"confirm","deletes":7,"message":"Stopped","extra":1}"#,
        )
        .unwrap();
        assert_eq!(
            (reply.status.as_str(), reply.deletes, reply.uploaded),
            ("confirm", 7, 0)
        );
    }
}

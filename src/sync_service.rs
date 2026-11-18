//! The readable Google Drive sync helper (`wros-sync`), run like the backup
//! helper: its own session, no terminal, cancellable, progress on stderr.
use crate::backup_service::{Job, Shared, run_helper, spawn_work};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

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
    #[serde(rename = "sync")]
    SyncWithTimeline {
        project: String,
        allow_deletes: bool,
        timeline: bool,
    },
}

impl Request {
    pub fn sync(project: String, allow_deletes: bool, timeline: bool) -> Self {
        Self::SyncWithTimeline {
            project,
            allow_deletes,
            timeline,
        }
    }
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
    /// Relative paths downloaded or removed locally.
    pub changed: Vec<String>,
    pub synced_at: Option<String>,
    pub drive_folder: Option<String>,
}

#[derive(Deserialize)]
struct Response<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

const UNAVAILABLE: &str = "Sync unavailable; install the WrOS Google Drive integration.";
// The helper takes the same operation lock for file sync and timeline calls.
static HELPER: Mutex<()> = Mutex::new(());

/// The installed helper, or the repository's in a debug build.
pub fn script() -> Option<PathBuf> {
    let installed = Path::new(wros_sys::paths::WROS_SYNC);
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
    spawn_after(move || request, script, env)
}

pub(crate) fn spawn_after(
    request: impl FnOnce() -> Request + Send + 'static,
    script: PathBuf,
    env: Vec<(&'static str, PathBuf)>,
) -> Result<Job<Reply>, String> {
    spawn_work("wros-sync", move |cancelled, progress| {
        let request = request();
        let _helper = crate::installed_session()
            .then(|| HELPER.lock().unwrap_or_else(|error| error.into_inner()));
        run(&request, &script, &env, cancelled, Some(progress))
    })
}

/// Run a timeline request from the timeline worker.
pub fn run_timeline(request: serde_json::Value) -> Result<serde_json::Value, String> {
    run_blocking(&request, None, &[], &AtomicBool::new(false))
}

/// Run the sync helper on an existing worker, keeping that worker's progress text.
pub(crate) fn run_blocking<T: DeserializeOwned>(
    request: &impl Serialize,
    script_override: Option<PathBuf>,
    env: &[(&str, PathBuf)],
    cancelled: &AtomicBool,
) -> Result<T, String> {
    let _helper = HELPER.try_lock().map_err(|_| "sync is already running")?;
    let script = script_override
        .or_else(script)
        .ok_or_else(|| UNAVAILABLE.to_owned())?;
    run(request, &script, env, cancelled, None)
}

fn run<T: DeserializeOwned>(
    request: &impl Serialize,
    script: &Path,
    env: &[(&str, PathBuf)],
    cancelled: &AtomicBool,
    progress: Option<Shared>,
) -> Result<T, String> {
    let input = serde_json::to_vec(request).map_err(|_| UNAVAILABLE.to_owned())?;
    let bytes = run_helper(
        script,
        &input,
        env,
        cancelled,
        progress,
        UNAVAILABLE,
        "Sync stopped; files not yet done are unchanged and sync again next time.",
    )?;
    let response: Response<T> =
        serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE.to_owned())?;
    if response.ok {
        response.data.ok_or_else(|| UNAVAILABLE.to_owned())
    } else {
        Err(response.error.unwrap_or_else(|| UNAVAILABLE.to_owned()))
    }
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
        assert!(reply.changed.is_empty());
    }

    #[test]
    fn sync_requests_send_the_timeline_flag() {
        for timeline in [true, false] {
            let request = Request::sync("/book".into(), true, timeline);
            assert_eq!(
                serde_json::to_value(request).unwrap(),
                serde_json::json!({
                    "operation": "sync",
                    "project": "/book",
                    "allow_deletes": true,
                    "timeline": timeline
                })
            );
        }
    }

    #[test]
    fn reply_parses_locally_changed_paths() {
        let response: Response<Reply> = serde_json::from_value(serde_json::json!({
            "ok": true,
            "data": {
                "status": "synced",
                "changed": ["manuscript/01.md", "notes/deleted.md"]
            }
        }))
        .unwrap();
        assert!(response.ok);
        assert_eq!(
            response.data.unwrap().changed,
            vec!["manuscript/01.md".to_owned(), "notes/deleted.md".to_owned()]
        );
    }
}

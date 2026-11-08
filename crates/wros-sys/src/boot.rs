//! The root helper reports the saved choice separately from boot-image preparation.
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub enabled: bool,
    pub pending: bool,
    pub failed: bool,
}

impl Status {
    pub fn preparation(&self) -> &'static str {
        if self.pending {
            "Preparing the start screen for the next restart."
        } else if self.failed {
            "Couldn't prepare the start screen. Choose Prepare the start screen again."
        } else {
            "The start screen takes the new colours at the next restart."
        }
    }
}

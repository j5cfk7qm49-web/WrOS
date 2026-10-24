//! Logical characters are already translated by Linux/XKB. Never remap prose.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    Qwerty,
    Colemak,
    ColemakDhAnsi,
    ColemakDhIso,
}

impl Layout {
    pub const ALL: [Self; 4] = [
        Self::Qwerty,
        Self::Colemak,
        Self::ColemakDhAnsi,
        Self::ColemakDhIso,
    ];
    pub fn id(&self) -> &'static str {
        match self {
            Self::Qwerty => "qwerty",
            Self::Colemak => "colemak",
            Self::ColemakDhAnsi => "colemak-dh-ansi",
            Self::ColemakDhIso => "colemak-dh-iso",
        }
    }
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
    pub fn parse(id: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|v| v.id() == id)
            .ok_or_else(|| anyhow::anyhow!("unregistered keyboard layout: {id}"))
    }
}

/// App-side preview state. The privileged helper owns an independent 15-second
/// timer; this model is only UI bookkeeping and remains safe if Grimoire dies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub token: String,
    pub layout: Layout,
    pub expires_at_ms: u64,
}
impl Preview {
    pub fn new(token: String, layout: Layout, now_ms: u64) -> Self {
        Self {
            token,
            layout,
            expires_at_ms: now_ms.saturating_add(15_000),
        }
    }
    pub fn remaining(&self, now_ms: u64) -> u64 {
        self.expires_at_ms.saturating_sub(now_ms).div_ceil(1000)
    }
    pub fn confirm(&self, now_ms: u64) -> Result<&str> {
        if now_ms >= self.expires_at_ms {
            bail!("keyboard preview expired; layout reverted");
        }
        Ok(&self.token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_deadline_does_not_depend_on_wall_clock() {
        let p = Preview::new("nonce".into(), Layout::ColemakDhAnsi, 500);
        assert_eq!(p.remaining(501), 15);
        assert_eq!(p.confirm(15_499).unwrap(), "nonce");
        assert!(p.confirm(15_500).is_err());
        assert_eq!(p.remaining(90_000), 0);
    }
    #[test]
    fn only_registered_ids_reach_the_helper() {
        for l in Layout::ALL {
            assert_eq!(Layout::parse(l.id()).unwrap(), l);
        }
        for s in ["../../etc/shadow", "us;reboot", "--help", ""] {
            assert!(Layout::parse(s).is_err());
        }
    }
}

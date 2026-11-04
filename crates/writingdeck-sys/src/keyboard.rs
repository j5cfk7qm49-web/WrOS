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
    pub fn label(&self) -> &'static str {
        match self {
            Self::Qwerty => "QWERTY (US standard)",
            Self::Colemak => "Colemak (home-row ARST / NEIO)",
            Self::ColemakDhAnsi => "Colemak-DH ANSI (US physical row)",
            Self::ColemakDhIso => "Colemak-DH ISO (angle-mod ISO key)",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Self::Qwerty => "Standard US QWERTY layout; home row ASDF / JKL;.",
            Self::Colemak => "Classic Colemak; home row ARST D H NEIO; preserves ZXCV positions.",
            Self::ColemakDhAnsi => {
                "Colemak-DH for standard ANSI boards; moves D and H to comfortable index-finger positions."
            }
            Self::ColemakDhIso => {
                "Colemak-DH for ISO boards with the extra angle-mod key beside Left Shift."
            }
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

/// A writer-named custom keyboard profile mapped strictly to one of the four
/// registered root-owned console keymaps (`Layout::ALL`). Arbitrary runtime
/// keymap paths (`.map`, `/`, `..`) are always refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomMapRegistration {
    pub name: String,
    pub base: Layout,
}

impl CustomMapRegistration {
    pub fn new(name: impl Into<String>, base: Layout) -> Result<Self> {
        let item = Self {
            name: name.into().trim().to_ascii_lowercase(),
            base,
        };
        item.validate()?;
        Ok(item)
    }

    pub fn validate(&self) -> Result<()> {
        self.base.validate()?;
        let n = self.name.as_str();
        if n.is_empty()
            || n.len() > 32
            || n.starts_with('-')
            || n.ends_with('-')
            || n.ends_with(".map")
            || !n
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            bail!(
                "custom map name must be 1..32 lowercase letters, digits or hyphens (arbitrary runtime keymap paths are refused)"
            );
        }
        if Layout::ALL.iter().any(|l| l.id() == n) {
            bail!("custom map name cannot shadow a built-in layout identifier");
        }
        Ok(())
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
    #[test]
    fn custom_map_registration_accepts_registered_choices_and_refuses_runtime_paths() {
        let reg = CustomMapRegistration::new("writer-ansi", Layout::ColemakDhAnsi).unwrap();
        assert_eq!(reg.name, "writer-ansi");
        assert_eq!(reg.base, Layout::ColemakDhAnsi);
        for bad in [
            "",
            "/usr/share/writingdeck/keymaps/qwerty.map",
            "../custom.map",
            "custom.map",
            "qwerty",
            "colemak",
            "-leading",
            "trailing-",
            "bad;shell",
        ] {
            assert!(
                CustomMapRegistration::new(bad, Layout::Qwerty).is_err(),
                "{bad:?}"
            );
        }
    }
}

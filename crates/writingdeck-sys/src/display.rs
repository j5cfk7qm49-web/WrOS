//! Manual display warmth; no theme changes or automatic manuscript rewriting.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warmth {
    pub enabled: bool,
    pub kelvin: u16,
    pub strength: u8,
}
impl Default for Warmth {
    fn default() -> Self {
        Self {
            enabled: true,
            kelvin: 2200,
            strength: 100,
        }
    }
}
impl Warmth {
    pub fn validate(self) -> Result<()> {
        if !(1000..=6500).contains(&self.kelvin) || self.strength > 100 {
            bail!("warmth requires 1000..6500 K and 0..100 percent strength");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    #[serde(flatten)]
    pub warmth: Warmth,
    pub available: bool,
    pub applied: bool,
    pub message: String,
}
impl Status {
    pub fn validate(&self) -> Result<()> {
        self.warmth.validate()?;
        if self.message.len() > 2048 || (!self.available && self.applied) {
            bail!("invalid display status reply");
        }
        Ok(())
    }
}
pub fn validate_token(token: &str) -> Result<()> {
    if token.len() != 32
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("invalid display preview token");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_choices_and_strict_status() {
        assert!(Warmth::default().validate().is_ok());
        assert!(
            Warmth {
                kelvin: 999,
                ..Warmth::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Warmth {
                strength: 101,
                ..Warmth::default()
            }
            .validate()
            .is_err()
        );
        let parsed: Status = serde_json::from_str(r#"{"enabled":false,"kelvin":2200,"strength":100,"available":false,"applied":false,"message":"unsupported"}"#).unwrap();
        parsed.validate().unwrap();
        assert!(validate_token(";reboot").is_err());
        assert!(validate_token(&"A".repeat(32)).is_err());
    }
}

//! Session-owned Wayland colour controls. Never reads input, prose or passwords.
use anyhow::{Result, bail};
use std::time::{Duration, Instant};
use wros_sys::display::Warmth;
mod process;
mod protocol;
mod runtime;
pub(crate) use process::ARGUMENT;
pub(crate) use runtime::backend;
pub(crate) use runtime::worker_main;
#[cfg(test)]
mod protocol_tests;

trait Screen {
    fn apply(&mut self, choice: Warmth) -> Result<()>;
    fn release(&mut self);
}

struct Preview {
    token: String,
    target: Warmth,
    deadline: Instant,
}

struct Transaction<S> {
    screen: S,
    saved: Warmth,
    pending: Option<Preview>,
}

impl<S: Screen> Transaction<S> {
    fn preview(&mut self, target: Warmth, token: String, now: Instant) -> Result<()> {
        target.validate()?;
        wros_sys::display::validate_token(&token)?;
        self.expire(now)?;
        if self.pending.is_some() {
            bail!("keep or cancel the current preview first");
        }
        // The actor owns this deadline independently of the writing worker.
        self.pending = Some(Preview {
            target,
            token,
            deadline: now + Duration::from_secs(15),
        });
        if let Err(error) = self.screen.apply(target) {
            self.pending = None;
            self.screen.release();
            return Err(error);
        }
        Ok(())
    }
    fn expire(&mut self, now: Instant) -> Result<()> {
        if self.pending.as_ref().is_some_and(|p| now >= p.deadline) {
            self.pending = None;
            self.screen.apply(self.saved)?;
        }
        Ok(())
    }
    fn resolve(
        &mut self,
        token: &str,
        keep: bool,
        now: Instant,
        save: impl FnOnce(Warmth, &str) -> Result<()>,
    ) -> Result<()> {
        wros_sys::display::validate_token(token)?;
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the preview has finished"))?;
        if pending.token != token {
            bail!("that request does not own the preview");
        }
        if now >= pending.deadline {
            self.expire(now)?;
            bail!("the preview expired and was reverted");
        }
        let pending = self.pending.take().unwrap();
        if keep {
            // Keep was accepted before expiry. A bounded durable save either
            // commits the choice or restores the last confirmed choice.
            if let Err(error) = save(pending.target, token) {
                self.screen.apply(self.saved)?;
                return Err(error);
            }
            self.saved = pending.target;
        } else {
            self.screen.apply(self.saved)?;
        }
        Ok(())
    }
}

fn ramps(points: u32, choice: Warmth) -> Result<Vec<u16>> {
    choice.validate()?;
    if !(2..=4096).contains(&points) {
        bail!("unsupported colour curve size");
    }
    let mut factors = [1.0; 3];
    if choice.enabled && choice.kelvin < 6500 && choice.strength > 0 {
        let relative = |wavelength: f64, temperature: f64| {
            let red = 610e-9;
            let constant = 0.01438776877;
            (red / wavelength).powi(5) * (constant / (red * temperature)).exp_m1()
                / (constant / (wavelength * temperature)).exp_m1()
        };
        for (index, wavelength, floor) in [(1, 550e-9, 0.10), (2, 460e-9, 0.02)] {
            let scale = (relative(wavelength, choice.kelvin as f64) / relative(wavelength, 6500.0))
                .clamp(floor, 1.0);
            factors[index] = 1.0 - (1.0 - scale) * choice.strength as f64 / 100.0;
        }
    }
    Ok(factors
        .into_iter()
        .flat_map(|factor| {
            (0..points)
                .map(move |n| (n as f64 * 65535.0 / (points - 1) as f64 * factor).round() as u16)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeScreen {
        choices: Vec<Warmth>,
        released: bool,
    }
    impl Screen for FakeScreen {
        fn apply(&mut self, choice: Warmth) -> Result<()> {
            self.choices.push(choice);
            Ok(())
        }
        fn release(&mut self) {
            self.released = true;
        }
    }
    fn transaction() -> Transaction<FakeScreen> {
        Transaction {
            screen: FakeScreen::default(),
            saved: Warmth {
                enabled: false,
                ..Warmth::default()
            },
            pending: None,
        }
    }

    #[test]
    fn unconfirmed_preview_expires_without_a_client_request() {
        let mut trial = transaction();
        let now = Instant::now();
        trial
            .preview(Warmth::default(), "a".repeat(32), now)
            .unwrap();
        assert_eq!(trial.screen.choices, [Warmth::default()]);
        trial.expire(now + Duration::from_secs(14)).unwrap();
        assert!(trial.pending.is_some());
        trial.expire(now + Duration::from_secs(15)).unwrap();
        assert!(trial.pending.is_none());
        assert_eq!(trial.screen.choices.last(), Some(&trial.saved));
        assert!(!trial.saved.enabled);
    }

    #[test]
    fn only_the_live_preview_can_commit_and_failed_saves_revert() {
        let mut trial = transaction();
        let now = Instant::now();
        let token = "a".repeat(32);
        trial
            .preview(Warmth::default(), token.clone(), now)
            .unwrap();
        assert!(
            trial
                .resolve(&"b".repeat(32), false, now, |_, _| panic!("no save"))
                .is_err()
        );
        assert!(trial.pending.is_some());
        assert!(
            trial
                .preview(Warmth::default(), token.clone(), now)
                .is_err()
        );
        assert!(
            trial
                .resolve(&token, true, now, |_, _| bail!(
                    "preferences changed externally"
                ))
                .is_err()
        );
        assert!(trial.pending.is_none());
        assert_eq!(trial.screen.choices.last(), Some(&trial.saved));
        trial
            .preview(Warmth::default(), token.clone(), now)
            .unwrap();
        trial
            .resolve(
                &token,
                true,
                now + Duration::from_secs(14),
                |choice, receipt| {
                    assert_eq!(choice, Warmth::default());
                    assert_eq!(receipt, token);
                    Ok(())
                },
            )
            .unwrap();
        trial.expire(now + Duration::from_secs(16)).unwrap();
        assert!(trial.saved.enabled);
        assert_eq!(trial.screen.choices.last(), Some(&trial.saved));
        assert!(
            trial
                .resolve(&token, true, now, |_, _| panic!("no save"))
                .is_err()
        );
    }

    #[test]
    fn expired_confirmation_restores_and_never_saves() {
        let mut trial = transaction();
        let now = Instant::now();
        let token = "a".repeat(32);
        trial
            .preview(Warmth::default(), token.clone(), now)
            .unwrap();
        assert!(
            trial
                .resolve(&token, true, now + Duration::from_secs(15), |_, _| panic!(
                    "expired"
                ))
                .is_err()
        );
        assert!(trial.pending.is_none());
        assert_eq!(trial.screen.choices.last(), Some(&trial.saved));
    }

    #[test]
    fn warmth_curves_are_bounded_monotone_and_do_not_accumulate() {
        let warm = Warmth::default();
        let values = ramps(256, warm).unwrap();
        assert_eq!(values.len(), 768);
        assert_eq!(values[255], 65535);
        assert!(values[511] < values[255]);
        assert!(values[767] < values[511]);
        for channel in values.chunks(256) {
            assert_eq!(channel[0], 0);
            assert!(channel.windows(2).all(|pair| pair[0] <= pair[1]));
        }
        assert_eq!(ramps(256, warm).unwrap(), values);
        assert!(ramps(1, warm).is_err());
        assert!(ramps(4097, warm).is_err());
        for unchanged in [
            Warmth {
                enabled: false,
                ..warm
            },
            Warmth {
                kelvin: 6500,
                ..warm
            },
            Warmth {
                strength: 0,
                ..warm
            },
        ] {
            let neutral = ramps(256, unchanged).unwrap();
            assert_eq!(&neutral[..256], &neutral[256..512]);
            assert_eq!(&neutral[..256], &neutral[512..]);
        }
    }
}

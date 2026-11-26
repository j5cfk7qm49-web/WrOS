//! The Pomodoro's two phase-change chimes.

use crate::scene::Phase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    WorkToBreak,
    BreakToWork,
}

pub(crate) fn on_phase_end(phase: Phase, enabled: bool, play: impl FnOnce(Kind)) {
    if enabled {
        match phase {
            Phase::Break => play(Kind::WorkToBreak),
            Phase::Focus => play(Kind::BreakToWork),
            Phase::Idle => {}
        }
    }
}

#[cfg(feature = "audio")]
pub(crate) fn ring(kind: Kind) {
    use std::num::{NonZeroU16, NonZeroU32};

    let _ = std::thread::Builder::new()
        .name("pomodoro-chime".into())
        .spawn(move || {
            let Ok(device) = rodio::DeviceSinkBuilder::open_default_sink() else {
                return;
            };
            let sink = rodio::Player::connect_new(device.mixer());
            sink.append(rodio::buffer::SamplesBuffer::new(
                NonZeroU16::MIN,
                NonZeroU32::new(SAMPLE_RATE).unwrap(),
                tones(kind),
            ));
            sink.sleep_until_end();
        });
}

#[cfg(not(feature = "audio"))]
pub(crate) fn ring(_kind: Kind) {}

#[cfg(any(feature = "audio", test))]
const SAMPLE_RATE: u32 = 48_000;
#[cfg(any(feature = "audio", test))]
const SAMPLES_PER_NOTE: u32 = SAMPLE_RATE * 350 / 1_000;

#[cfg(any(feature = "audio", test))]
fn tones(kind: Kind) -> Vec<f32> {
    let frequencies = match kind {
        Kind::WorkToBreak => [880.0, 660.0],
        Kind::BreakToWork => [660.0, 880.0],
    };
    frequencies
        .into_iter()
        .flat_map(|hz| {
            (0..SAMPLES_PER_NOTE).map(move |sample| {
                let time = sample as f32 / SAMPLE_RATE as f32;
                let attack = (time / 0.01).min(1.0);
                let decay = (-15.0 * time).exp();
                0.25 * attack * decay * (std::f32::consts::TAU * hz * time).sin()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_is_not_called_when_the_pomodoro_chime_is_off() {
        for phase in [Phase::Break, Phase::Focus] {
            on_phase_end(phase, false, |_| panic!("the chime is off"));
        }
    }

    #[test]
    fn phase_end_chooses_the_chime_for_the_next_phase() {
        let mut played = Vec::new();
        on_phase_end(Phase::Break, true, |kind| played.push(kind));
        on_phase_end(Phase::Focus, true, |kind| played.push(kind));
        on_phase_end(Phase::Idle, true, |kind| played.push(kind));
        assert_eq!(played, [Kind::WorkToBreak, Kind::BreakToWork]);
    }

    #[test]
    fn chime_tones_last_350_ms_each_and_stay_within_quarter_amplitude() {
        let descending = tones(Kind::WorkToBreak);
        let ascending = tones(Kind::BreakToWork);
        for samples in [&descending, &ascending] {
            assert_eq!(samples.len(), 33_600);
            assert!(
                samples
                    .iter()
                    .all(|&sample| (-0.25..=0.25).contains(&sample))
            );
            assert!(samples.iter().any(|&sample| sample.abs() > 0.1));
        }
        assert_eq!(descending[..16_800], ascending[16_800..]);
        assert_eq!(descending[16_800..], ascending[..16_800]);
    }
}

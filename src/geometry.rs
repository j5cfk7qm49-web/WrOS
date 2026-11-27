#![warn(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

pub(crate) fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

pub(crate) fn to_u16_i(n: i32) -> u16 {
    u16::try_from(n.max(0)).unwrap_or(u16::MAX)
}

#[cfg(any(test, all(target_os = "linux", feature = "gui-wgpu")))]
pub(crate) fn to_u16_f(x: f32) -> u16 {
    to_u16_f64(f64::from(x))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the bounds map NaN to zero and keep the value in the u16 range; the cast truncates fractions"
)]
pub(crate) fn to_u16_f64(x: f64) -> u16 {
    x.max(0.0).min(f64::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usize_to_u16_saturates() {
        assert_eq!(to_u16(0), 0);
        assert_eq!(to_u16(42), 42);
        assert_eq!(to_u16(usize::from(u16::MAX)), u16::MAX);
        assert_eq!(to_u16(usize::MAX), u16::MAX);
    }

    #[test]
    fn i32_to_u16_saturates_at_both_ends() {
        assert_eq!(to_u16_i(0), 0);
        assert_eq!(to_u16_i(42), 42);
        assert_eq!(to_u16_i(i32::from(u16::MAX) + 1), u16::MAX);
        assert_eq!(to_u16_i(-1), 0);
    }

    #[test]
    fn f32_to_u16_truncates_and_saturates() {
        assert_eq!(to_u16_f(0.0), 0);
        assert_eq!(to_u16_f(42.9), 42);
        assert_eq!(to_u16_f(65_536.0), u16::MAX);
        assert_eq!(to_u16_f(-1.0), 0);
        assert_eq!(to_u16_f(f32::NAN), 0);
    }
}

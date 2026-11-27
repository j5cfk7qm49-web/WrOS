pub(crate) fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

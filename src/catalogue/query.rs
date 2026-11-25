//! Classification and identifier validation for catalogue queries.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Isbn(String),
    Doi(String),
    Address(String),
    Text(String),
}

pub fn classify(line: &str) -> Option<Query> {
    let line = line.trim();
    if line.chars().filter(|c| !c.is_whitespace()).nth(2).is_none() {
        return None;
    }
    if let Some(id) = isbn(line) {
        return Some(Query::Isbn(id));
    }
    if let Some(id) = doi(line) {
        return Some(Query::Doi(id));
    }
    if is_address(line) {
        return Some(Query::Address(line.to_owned()));
    }
    Some(Query::Text(line.to_owned()))
}

pub fn isbn(input: &str) -> Option<String> {
    let digits: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    valid_isbn(&digits).then_some(digits)
}

pub fn doi(input: &str) -> Option<String> {
    let input = input.trim();
    let doi = input.strip_prefix("https://doi.org/").unwrap_or(input);
    let doi = doi.strip_prefix("doi:").unwrap_or(doi).trim();
    let (prefix, suffix) = doi.split_once('/')?;
    let digits = prefix.strip_prefix("10.")?;
    let valid = (4..=9).contains(&digits.len())
        && digits.bytes().all(|b| b.is_ascii_digit())
        && !suffix.is_empty()
        && doi.len() <= 256
        && !doi.chars().any(|c| c.is_whitespace() || c.is_control());
    valid.then(|| doi.to_owned())
}

fn is_address(input: &str) -> bool {
    let Some((scheme, rest)) = input.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_and_port = authority.rsplit('@').next().unwrap_or_default();
    let host = host_and_port.split(':').next().unwrap_or_default();
    !host.trim_end_matches('.').eq_ignore_ascii_case("doi.org")
}

fn valid_isbn(value: &str) -> bool {
    let length = value.len();
    let modulus = match length {
        10 => 11,
        13 => 10,
        _ => return false,
    };
    let mut checksum = 0;
    for (index, digit) in value.bytes().enumerate() {
        let number = match digit {
            b'0'..=b'9' => usize::from(digit - b'0'),
            b'X' if length == 10 && index == 9 => 10,
            _ => return false,
        };
        checksum += number
            * if length == 10 {
                10 - index
            } else {
                1 + 2 * (index % 2)
            };
    }
    checksum.is_multiple_of(modulus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognises_each_kind() {
        assert_eq!(
            classify("978-0-14-032872-1"),
            Some(Query::Isbn("9780140328721".into()))
        );
        assert_eq!(
            classify("0140621806"),
            Some(Query::Isbn("0140621806".into()))
        );
        assert_eq!(
            classify("978-0-14-032872-2"),
            Some(Query::Text("978-0-14-032872-2".into()))
        );
        assert_eq!(
            classify("doi:10.2307/40149212"),
            Some(Query::Doi("10.2307/40149212".into()))
        );
        assert_eq!(
            classify("https://doi.org/10.2307/40149212"),
            Some(Query::Doi("10.2307/40149212".into()))
        );
        assert_eq!(
            classify("https://example.com/a"),
            Some(Query::Address("https://example.com/a".into()))
        );
        assert_eq!(
            classify("le guin dispossessed"),
            Some(Query::Text("le guin dispossessed".into()))
        );
        assert_eq!(classify("le"), None);
    }
}

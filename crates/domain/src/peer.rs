use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId(String);

impl PeerId {
    pub fn new(value: impl Into<String>) -> Self {
        PeerId(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatrixUserId(String);

impl MatrixUserId {
    pub fn new(value: impl Into<String>) -> Self {
        MatrixUserId(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MatrixUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhoneNumber(String);

impl PhoneNumber {
    pub fn new(value: impl AsRef<str>) -> Self {
        let raw = value.as_ref();
        let mut out = String::with_capacity(raw.len());
        for (i, c) in raw.chars().enumerate() {
            match c {
                '+' if i == 0 => out.push('+'),
                c if c.is_ascii_digit() => out.push(c),
                _ => {}
            }
        }
        PhoneNumber(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PhoneNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConversationId(String);

impl ConversationId {
    pub fn new(value: impl Into<String>) -> Self {
        ConversationId(value.into())
    }

    pub fn with_peer(peer: &PeerId) -> Self {
        ConversationId(peer.as_str().to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SmsRowId(String);

impl SmsRowId {
    pub fn new(value: impl Into<String>) -> Self {
        SmsRowId(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SmsRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_numbers_normalise_to_a_comparable_form() {
        let typed = PhoneNumber::new("+33 6 12-34.56 78");
        let from_carrier = PhoneNumber::new("+33612345678");
        assert_eq!(typed, from_carrier);
    }

    #[test]
    fn a_plus_is_only_significant_in_leading_position() {
        assert_eq!(PhoneNumber::new("336+12").as_str(), "33612");
    }

    #[test]
    fn conversation_id_derives_from_the_peer() {
        let peer = PeerId::new("bob");
        assert_eq!(ConversationId::with_peer(&peer).as_str(), "bob");
    }
}

use core::fmt;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const MESSAGE_ID_LEN: usize = 26;
pub const SHORT_TAG_LEN: usize = 13;
const RANDOM_BITS: u32 = 80;
pub const MAX_TIMESTAMP_MS: u64 = (1 << 48) - 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdParseError {
    BadLength { expected: usize, found: usize },
    BadCharacter(char),
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdParseError::BadLength { expected, found } => {
                write!(f, "expected {expected} characters, found {found}")
            }
            IdParseError::BadCharacter(c) => write!(f, "invalid base32 character {c:?}"),
        }
    }
}

impl std::error::Error for IdParseError {}

fn decode_char(c: char) -> Result<u8, IdParseError> {
    match c.to_ascii_uppercase() {
        c @ '0'..='9' => Ok(c as u8 - b'0'),
        'I' | 'L' => Ok(1),
        'O' => Ok(0),
        c @ ('A'..='H' | 'J' | 'K' | 'M' | 'N' | 'P'..='T' | 'V'..='Z') => {
            Ok(ALPHABET
                .iter()
                .position(|&a| a == c as u8)
                .expect("character is in the alphabet") as u8)
        }
        _ => Err(IdParseError::BadCharacter(c)),
    }
}

fn encode_u128(value: u128, chars: usize) -> String {
    let mut buf = vec![0u8; chars];
    for (i, slot) in buf.iter_mut().enumerate() {
        let shift = 5 * (chars - 1 - i);
        *slot = ALPHABET[((value >> shift) & 0x1f) as usize];
    }
    String::from_utf8(buf).expect("alphabet is ASCII")
}

fn decode_u128(text: &str, chars: usize) -> Result<u128, IdParseError> {
    if text.chars().count() != chars {
        return Err(IdParseError::BadLength {
            expected: chars,
            found: text.chars().count(),
        });
    }
    let mut value: u128 = 0;
    for c in text.chars() {
        value = (value << 5) | u128::from(decode_char(c)?);
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(u128);

impl MessageId {
    pub fn from_parts(timestamp_ms: u64, random: u128) -> Self {
        let ts = u128::from(timestamp_ms & MAX_TIMESTAMP_MS);
        let rand = random & ((1u128 << RANDOM_BITS) - 1);
        MessageId((ts << RANDOM_BITS) | rand)
    }

    pub const fn from_u128(raw: u128) -> Self {
        MessageId(raw)
    }

    pub const fn as_u128(self) -> u128 {
        self.0
    }

    pub const fn timestamp_ms(self) -> u64 {
        (self.0 >> RANDOM_BITS) as u64
    }

    pub const fn short_tag(self) -> ShortTag {
        ShortTag(self.0 as u64)
    }

    pub fn to_base32(self) -> String {
        encode_u128(self.0, MESSAGE_ID_LEN)
    }

    pub fn parse(text: &str) -> Result<Self, IdParseError> {
        decode_u128(text, MESSAGE_ID_LEN).map(MessageId)
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_base32())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShortTag(u64);

impl ShortTag {
    pub const fn from_u64(raw: u64) -> Self {
        ShortTag(raw)
    }
    pub const fn as_u64(self) -> u64 {
        self.0
    }
    pub fn to_base32(self) -> String {
        encode_u128(u128::from(self.0), SHORT_TAG_LEN)
    }

    pub fn parse(text: &str) -> Result<Self, IdParseError> {
        decode_u128(text, SHORT_TAG_LEN).map(|v| ShortTag(v as u64))
    }
}

impl fmt::Display for ShortTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_base32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_id_round_trips_through_base32() {
        let id = MessageId::from_parts(1_754_800_000_000, 0x0123_4567_89AB_CDEF_0123);
        let text = id.to_base32();
        assert_eq!(text.len(), MESSAGE_ID_LEN);
        assert_eq!(MessageId::parse(&text), Ok(id));
    }

    #[test]
    fn short_tag_round_trips_through_base32() {
        let tag = MessageId::from_parts(1_754_800_000_000, 0xDEAD_BEEF_CAFE_F00D_1234).short_tag();
        let text = tag.to_base32();
        assert_eq!(text.len(), SHORT_TAG_LEN);
        assert_eq!(ShortTag::parse(&text), Ok(tag));
    }

    #[test]
    fn short_tag_is_derivable_from_the_full_id() {
        let id = MessageId::from_parts(1_754_800_000_000, 0x1111_2222_3333_4444_5555);
        let recovered = MessageId::parse(&id.to_base32()).unwrap();
        assert_eq!(recovered.short_tag(), id.short_tag());
    }

    #[test]
    fn ids_sort_by_compose_time() {
        let early = MessageId::from_parts(1_000, u128::MAX);
        let late = MessageId::from_parts(2_000, 0);
        assert!(early < late, "timestamp must dominate the random tail");
    }

    #[test]
    fn timestamp_survives_the_round_trip() {
        let id = MessageId::from_parts(1_754_800_000_000, 42);
        assert_eq!(id.timestamp_ms(), 1_754_800_000_000);
    }

    #[test]
    fn parse_rejects_wrong_length() {
        assert_eq!(
            MessageId::parse("ABC"),
            Err(IdParseError::BadLength {
                expected: MESSAGE_ID_LEN,
                found: 3
            })
        );
    }

    #[test]
    fn parse_rejects_characters_outside_the_alphabet() {
        let mut text = MessageId::from_parts(1, 1).to_base32();
        text.replace_range(0..1, "-");
        assert_eq!(MessageId::parse(&text), Err(IdParseError::BadCharacter('-')));
    }

    #[test]
    fn parse_accepts_confusable_substitutions() {
        let tag = ShortTag::from_u64(0b1_00001);
        let text = tag.to_base32();
        let confused = text.replace('1', "I").replace('0', "O");
        assert_eq!(ShortTag::parse(&confused), Ok(tag));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let id = MessageId::from_parts(1_754_800_000_000, 0xABCD_EF01_2345_6789_ABCD);
        assert_eq!(MessageId::parse(&id.to_base32().to_lowercase()), Ok(id));
    }

    #[test]
    fn from_parts_truncates_rather_than_panicking() {
        let id = MessageId::from_parts(u64::MAX, u128::MAX);
        assert_eq!(id.timestamp_ms(), MAX_TIMESTAMP_MS);
    }
}

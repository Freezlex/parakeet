use crate::id::{ShortTag, SHORT_TAG_LEN};

pub const TRAILER_PREFIX: &str = "#pk:";

pub const TRAILER_CHARS: usize = 1 + 4 + SHORT_TAG_LEN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framed {
    pub body: String,

    pub tag: Option<ShortTag>,
}

pub fn encode(body: &str, tag: ShortTag) -> String {
    format!("{body}\n{TRAILER_PREFIX}{}", tag.to_base32())
}

pub fn decode(text: &str) -> Framed {
    match split_trailer(text) {
        Some((body, tag)) => Framed {
            body: body.to_owned(),
            tag: Some(tag),
        },
        None => Framed {
            body: text.to_owned(),
            tag: None,
        },
    }
}

fn split_trailer(text: &str) -> Option<(&str, ShortTag)> {
    let (body, last_line) = match text.rfind('\n') {
        Some(nl) => (&text[..nl], &text[nl + 1..]),
        None => ("", text),
    };

    let tag_text = last_line.strip_prefix(TRAILER_PREFIX)?;
    if tag_text.len() != SHORT_TAG_LEN {
        return None;
    }
    ShortTag::parse(tag_text).ok().map(|tag| (body, tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::MessageId;

    fn tag() -> ShortTag {
        MessageId::from_parts(1_754_800_000_000, 0xDEAD_BEEF_CAFE_F00D_1234).short_tag()
    }

    #[test]
    fn encode_then_decode_recovers_both_halves() {
        let framed = decode(&encode("Hey, are we still on for tonight?", tag()));
        assert_eq!(framed.body, "Hey, are we still on for tonight?");
        assert_eq!(framed.tag, Some(tag()));
    }

    #[test]
    fn the_trailer_costs_what_we_claim() {
        let encoded = encode("", tag());
        assert_eq!(encoded.chars().count(), TRAILER_CHARS);
    }

    #[test]
    fn a_plain_sms_decodes_to_its_whole_text() {
        let framed = decode("running late, sorry!");
        assert_eq!(framed.body, "running late, sorry!");
        assert_eq!(framed.tag, None);
    }

    #[test]
    fn a_multiline_body_keeps_its_newlines() {
        let framed = decode(&encode("line one\nline two", tag()));
        assert_eq!(framed.body, "line one\nline two");
        assert_eq!(framed.tag, Some(tag()));
    }

    #[test]
    fn a_trailer_shaped_token_mid_line_is_left_alone() {
        let text = "look at this #pk:7K2M9QX4RTB0F";
        let framed = decode(text);
        assert_eq!(framed.body, text);
        assert_eq!(framed.tag, None);
    }

    #[test]
    fn a_malformed_tag_is_treated_as_text() {
        for bad in [
            "hi\n#pk:SHORT",
            "hi\n#pk:7K2M9QX4RTB0FEXTRA",
            "hi\n#pk:7K2M9QX4RTB0!",
            "hi\n#pk:",
            "hi\n#PK:7K2M9QX4RTB0F",
        ] {
            let framed = decode(bad);
            assert_eq!(framed.body, bad, "should not have stripped {bad:?}");
            assert_eq!(framed.tag, None);
        }
    }

    #[test]
    fn an_empty_body_still_carries_its_tag() {
        let framed = decode(&encode("", tag()));
        assert_eq!(framed.body, "");
        assert_eq!(framed.tag, Some(tag()));
    }

    #[test]
    fn decoding_is_idempotent_on_already_stripped_text() {
        let once = decode(&encode("hello", tag()));
        let twice = decode(&once.body);
        assert_eq!(twice.body, "hello");
        assert_eq!(twice.tag, None);
    }
}

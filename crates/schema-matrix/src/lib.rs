use domain::MessageId;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "im.polarys.parakeet";

pub const TXN_PREFIX: &str = "pk-";

pub const VIA_SMS_FALLBACK: &str = "sms_fallback";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Via {
    #[default]
    Direct,

    SmsFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageContent {
    pub msgtype: String,
    pub body: String,

    #[serde(rename = "im.polarys.parakeet.id", skip_serializing_if = "Option::is_none")]
    pub parakeet_id: Option<String>,

    #[serde(
        rename = "im.polarys.parakeet.origin_ts",
        skip_serializing_if = "Option::is_none"
    )]
    pub parakeet_origin_ts: Option<u64>,

    #[serde(rename = "im.polarys.parakeet.via", skip_serializing_if = "Option::is_none")]
    pub parakeet_via: Option<String>,
}

impl MessageContent {
    pub fn new(body: impl Into<String>, id: MessageId, origin_ts: u64, via: Via) -> Self {
        MessageContent {
            msgtype: "m.text".to_owned(),
            body: body.into(),
            parakeet_id: Some(id.to_base32()),
            parakeet_origin_ts: Some(origin_ts),
            parakeet_via: match via {
                Via::Direct => None,
                Via::SmsFallback => Some(VIA_SMS_FALLBACK.to_owned()),
            },
        }
    }

    pub fn message_id(&self) -> Option<MessageId> {
        MessageId::parse(self.parakeet_id.as_deref()?).ok()
    }

    pub fn origin_ts_or(&self, server_ts: u64) -> u64 {
        self.parakeet_origin_ts.unwrap_or(server_ts)
    }

    pub fn via(&self) -> Via {
        match self.parakeet_via.as_deref() {
            Some(VIA_SMS_FALLBACK) => Via::SmsFallback,
            _ => Via::Direct,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("message content always serialises")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

pub fn txn_id_for(id: MessageId) -> String {
    format!("{TXN_PREFIX}{}", id.to_base32())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> MessageId {
        MessageId::from_parts(1_754_800_000_000, 0xDEAD_BEEF_CAFE_F00D_1234)
    }

    #[test]
    fn content_round_trips_through_json() {
        let content = MessageContent::new("hi", id(), 1_754_800_000_000, Via::SmsFallback);
        let decoded = MessageContent::from_json(&content.to_json()).unwrap();
        assert_eq!(decoded, content);
        assert_eq!(decoded.message_id(), Some(id()));
        assert_eq!(decoded.via(), Via::SmsFallback);
    }

    #[test]
    fn the_wire_format_uses_the_documented_keys() {
        let json = MessageContent::new("hi", id(), 42, Via::Direct).to_json();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["msgtype"], "m.text");
        assert_eq!(value["body"], "hi");
        assert_eq!(value["im.polarys.parakeet.id"], id().to_base32());
        assert_eq!(value["im.polarys.parakeet.origin_ts"], 42);
        assert!(
            value.get("im.polarys.parakeet.via").is_none(),
            "a direct send should not claim to be a fallback"
        );
    }

    #[test]
    fn an_event_from_a_plain_matrix_client_still_decodes() {
        let decoded =
            MessageContent::from_json(r#"{"msgtype":"m.text","body":"sent from Element"}"#).unwrap();
        assert_eq!(decoded.body, "sent from Element");
        assert_eq!(decoded.message_id(), None);
        assert_eq!(decoded.via(), Via::Direct);
        assert_eq!(decoded.origin_ts_or(1_234), 1_234);
    }

    #[test]
    fn a_malformed_id_is_treated_as_absent() {
        let mut content = MessageContent::new("hi", id(), 42, Via::Direct);
        content.parakeet_id = Some("not-an-id".to_owned());
        assert_eq!(content.message_id(), None);
    }

    #[test]
    fn origin_ts_beats_the_server_timestamp() {
        let content = MessageContent::new("hi", id(), 1_000, Via::SmsFallback);
        assert_eq!(content.origin_ts_or(9_999_999), 1_000);
    }

    #[test]
    fn the_transaction_id_is_stable_for_an_id() {
        assert_eq!(txn_id_for(id()), txn_id_for(id()));
        assert!(txn_id_for(id()).starts_with(TXN_PREFIX));
        assert_ne!(txn_id_for(id()), txn_id_for(MessageId::from_parts(1, 1)));
    }

    #[test]
    fn unknown_extra_keys_do_not_break_decoding() {
        let decoded = MessageContent::from_json(
            r#"{"msgtype":"m.text","body":"hi","m.mentions":{},"im.nheko.x":1}"#,
        )
        .unwrap();
        assert_eq!(decoded.body, "hi");
    }
}

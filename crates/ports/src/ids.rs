use domain::{MessageId, ShortTag};

pub trait IdGen: Send + Sync {
    fn mint(&self) -> MessageId;
    fn mint_tag(&self) -> ShortTag {
        self.mint().short_tag()
    }
}

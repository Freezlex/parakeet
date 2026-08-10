use domain::{MatrixUserId, PeerId, PhoneNumber};

// Not a big fan of this but saw the idea on someone else project... don't know how to merge peer reliabily
// with matrix id and phone number, so let's just keep a directory of contacts for now.
pub trait Directory: Send + Sync {
    fn matrix_id(&self, peer: &PeerId) -> Option<MatrixUserId>;
    fn phone(&self, peer: &PeerId) -> Option<PhoneNumber>;
    fn peer_by_matrix_id(&self, id: &MatrixUserId) -> Option<PeerId>;
    fn peer_by_phone(&self, phone: &PhoneNumber) -> Option<PeerId>;
    fn display_name(&self, peer: &PeerId) -> String {
        peer.as_str().to_owned()
    }
}

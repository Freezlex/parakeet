use std::sync::{Arc, Mutex};

use domain::{MatrixUserId, PeerId, PhoneNumber};
use ports::Directory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub peer: PeerId,
    pub display_name: String,
    pub matrix_id: MatrixUserId,
    pub phone: PhoneNumber,
}

impl Contact {
    pub fn new(
        peer: impl Into<String>,
        display_name: impl Into<String>,
        matrix_id: impl Into<String>,
        phone: impl AsRef<str>,
    ) -> Self {
        Contact {
            peer: PeerId::new(peer),
            display_name: display_name.into(),
            matrix_id: MatrixUserId::new(matrix_id),
            phone: PhoneNumber::new(phone),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryDirectory {
    contacts: Arc<Mutex<Vec<Contact>>>,
}

impl MemoryDirectory {
    pub fn new(contacts: impl IntoIterator<Item = Contact>) -> Self {
        MemoryDirectory {
            contacts: Arc::new(Mutex::new(contacts.into_iter().collect())),
        }
    }

    pub fn add(&self, contact: Contact) {
        let mut guard = self.contacts.lock().expect("directory lock");
        guard.retain(|c| c.peer != contact.peer);
        guard.push(contact);
    }

    pub fn contacts(&self) -> Vec<Contact> {
        self.contacts.lock().expect("directory lock").clone()
    }

    fn find<F>(&self, matches: F) -> Option<Contact>
    where
        F: Fn(&Contact) -> bool,
    {
        self.contacts
            .lock()
            .expect("directory lock")
            .iter()
            .find(|c| matches(c))
            .cloned()
    }
}

impl Directory for MemoryDirectory {
    fn matrix_id(&self, peer: &PeerId) -> Option<MatrixUserId> {
        self.find(|c| &c.peer == peer).map(|c| c.matrix_id)
    }

    fn phone(&self, peer: &PeerId) -> Option<PhoneNumber> {
        self.find(|c| &c.peer == peer).map(|c| c.phone)
    }

    fn peer_by_matrix_id(&self, id: &MatrixUserId) -> Option<PeerId> {
        self.find(|c| &c.matrix_id == id).map(|c| c.peer)
    }

    fn peer_by_phone(&self, phone: &PhoneNumber) -> Option<PeerId> {
        self.find(|c| &c.phone == phone).map(|c| c.peer)
    }

    fn display_name(&self, peer: &PeerId) -> String {
        self.find(|c| &c.peer == peer)
            .map(|c| c.display_name)
            .unwrap_or_else(|| peer.as_str().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> MemoryDirectory {
        MemoryDirectory::new([Contact::new(
            "bob",
            "Bob",
            "@bob:matriX.org",
            "+33612345678",
        )])
    }

    #[test]
    fn the_same_person_is_reachable_from_either_address() {
        let dir = directory();
        let by_phone = dir.peer_by_phone(&PhoneNumber::new("+33612345678")).unwrap();
        let by_matrix = dir
            .peer_by_matrix_id(&MatrixUserId::new("@bob:matrix.org"))
            .unwrap();
        assert_eq!(by_phone, by_matrix);
    }

    #[test]
    fn lookups_tolerate_carrier_formatting() {
        let dir = directory();
        assert!(dir
            .peer_by_phone(&PhoneNumber::new("+33 6 12 34 56 78"))
            .is_some());
    }

    #[test]
    fn an_unknown_address_resolves_to_nothing() {
        let dir = directory();
        assert!(dir.peer_by_phone(&PhoneNumber::new("+33699999999")).is_none());
    }

    #[test]
    fn adding_a_contact_twice_replaces_it() {
        let dir = directory();
        dir.add(Contact::new("bob", "Bobby", "@bob:matrix.org", "+33612345678"));
        assert_eq!(dir.contacts().len(), 1);
        assert_eq!(dir.display_name(&PeerId::new("bob")), "Bobby");
    }

    #[test]
    fn an_unknown_peer_displays_as_its_id() {
        assert_eq!(directory().display_name(&PeerId::new("karen")), "karen"); // F*ck you, Karen.
    }

}

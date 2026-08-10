#![allow(dead_code)]

use std::sync::Arc;

use app::{Client, ClientParts};
use domain::{ConversationId, NetStatus, PeerId, PhoneNumber};
use infra_matrix_fake::{FakeHomeserver, FakeMatrixTransport};
use infra_memory_store::{
    Contact, ManualClock, MemoryConnectivity, MemoryDirectory, MemoryStore, SequentialIdGen,
};
use infra_sms_fake::{FakeSmsNetwork, FakeSmsTransport, StoredSms};

pub const ALICE_PHONE: &str = "+33600000001";
pub const BOB_PHONE: &str = "+33600000002";
pub const STRANGER_PHONE: &str = "+33600000009";

pub struct Device {
    pub client: Client,
    pub matrix: FakeMatrixTransport,
    pub sms: FakeSmsTransport,
    pub store: MemoryStore,
    pub phone: PhoneNumber,
}

impl Device {
    pub fn lose_data(&self) {
        self.matrix.set_connected(false);
        self.client.set_connectivity(NetStatus::Offline);
    }

    pub fn regain_data(&self) {
        self.matrix.set_connected(true);
        self.client.set_connectivity(NetStatus::Online);
    }

    pub fn lose_all_service(&self) {
        self.lose_data();
        self.sms.set_reachable(false);
    }

    pub fn sms_storage(&self) -> Vec<StoredSms> {
        self.sms.stored_rows()
    }

    pub fn thread(&self, with: &str) -> Vec<app::MessageView> {
        self.client.thread(&ConversationId::new(with))
    }
}

pub struct World {
    pub homeserver: FakeHomeserver,
    pub carrier: FakeSmsNetwork,
    pub clock: Arc<ManualClock>,
    pub alice: Device,
    pub bob: Device,
}

impl World {
    pub fn new() -> Self {
        let clock = Arc::new(ManualClock::new(1_754_800_000_000));
        let homeserver = FakeHomeserver::with_clock(clock.clone());
        let carrier = FakeSmsNetwork::with_clock(clock.clone());

        let contacts = [
            Contact::new("alice", "Alice", "@alice:matrix.org", ALICE_PHONE),
            Contact::new("bob", "Bob", "@bob:matrix.org", BOB_PHONE),
        ];

        let alice = build_device("alice", &homeserver, &carrier, clock.clone(), 1, contacts.clone());
        let bob = build_device("bob", &homeserver, &carrier, clock.clone(), 2, contacts);

        World {
            homeserver,
            carrier,
            clock,
            alice,
            bob,
        }
    }

    pub fn stranger(&self) -> FakeSmsTransport {
        self.carrier.handset(STRANGER_PHONE)
    }
}

fn build_device(
    who: &str,
    homeserver: &FakeHomeserver,
    carrier: &FakeSmsNetwork,
    clock: Arc<ManualClock>,
    seed: u64,
    contacts: impl IntoIterator<Item = Contact>,
) -> Device {
    let directory = MemoryDirectory::new(contacts);
    let peer = PeerId::new(who);
    let matrix_id = directory
        .contacts()
        .into_iter()
        .find(|c| c.peer == peer)
        .expect("device knows itself");

    let matrix = homeserver.client(matrix_id.matrix_id.as_str());
    let sms = carrier.handset(matrix_id.phone.as_str());
    let net = Arc::new(MemoryConnectivity::new(NetStatus::Online));
    let store = MemoryStore::new();

    let client = Client::new(ClientParts {
        me: peer,
        matrix: Arc::new(matrix.clone()),
        sms: Arc::new(sms.clone()),
        store: Arc::new(store.clone()),
        net: net.clone(),
        clock: clock.clone(),
        ids: Arc::new(SequentialIdGen::new(clock, seed)),
        directory: Arc::new(directory),
    });

    Device {
        client,
        matrix,
        sms,
        store,
        phone: matrix_id.phone,
    }
}

pub fn trailer_for(id: domain::MessageId) -> String {
    format!("#pk:{}", id.short_tag().to_base32())
}

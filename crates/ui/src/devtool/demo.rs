use std::sync::Arc;

use app::{Client, ClientParts, MessageView};
use domain::{ConversationId, NetStatus, PeerId};
use infra_matrix_fake::{FakeHomeserver, FakeMatrixTransport};
use infra_memory_store::{
    Contact, EntropyIdGen, MemoryConnectivity, MemoryDirectory, MemoryStore, SystemClock,
};
use infra_sms_fake::{FakeSmsNetwork, FakeSmsTransport, StoredSms};

pub const ALICE_PHONE: &str = "+33600000001"; // Later to test phone resolution after matrix ID
pub const BOB_PHONE: &str = "+33600000002";

pub struct DemoDevice {
    pub name: String,
    pub me: PeerId,
    pub client: Client,
    pub matrix: FakeMatrixTransport,
    pub sms: FakeSmsTransport,
}

impl DemoDevice {
    pub fn messages(&self, peer: &PeerId) -> Vec<MessageView> {
        self.client.thread(&ConversationId::with_peer(peer))
    }

    pub fn sms_storage(&self) -> Vec<StoredSms> {
        self.sms.stored_rows()
    }

    pub fn set_online(&self, online: bool) {
        self.matrix.set_connected(online);
        self.client.set_connectivity(if online {
            NetStatus::Online
        } else {
            NetStatus::Offline
        });
    }

    pub fn is_online(&self) -> bool {
        self.client.connectivity() != NetStatus::Offline
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoContact {
    pub peer: PeerId,
    pub display_name: String,
    pub phone: String,
}

pub struct DemoWorld {
    pub homeserver: FakeHomeserver,
    pub carrier: FakeSmsNetwork,
    pub alice: DemoDevice,
    pub bob: DemoDevice,
}

impl Default for DemoWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoWorld {
    pub fn new() -> Self {
        let clock = Arc::new(SystemClock);
        let homeserver = FakeHomeserver::with_clock(clock.clone());
        let carrier = FakeSmsNetwork::with_clock(clock.clone());

        let contacts = [
            Contact::new("alice", "Alice", "@alice:matrix.org", ALICE_PHONE),
            Contact::new("bob", "Bob", "@bob:matrix.org", BOB_PHONE),
        ];

        let alice = build("alice", "Alice", &homeserver, &carrier, &contacts);
        let bob = build("bob", "Bob", &homeserver, &carrier, &contacts);
        DemoWorld {
            homeserver,
            carrier,
            alice,
            bob,
        }
    }

    pub fn contacts(&self) -> Vec<DemoContact> {
        vec![DemoContact {
            peer: self.bob.me.clone(),
            display_name: self.bob.name.clone(),
            phone: BOB_PHONE.to_owned(),
        }]
    }

    pub fn peer_sms_storage(&self, peer: &PeerId) -> Vec<StoredSms> {
        if peer == &self.bob.me {
            self.bob.sms_storage()
        } else {
            Vec::new()
        }
    }

    pub fn server_is_up(&self) -> bool {
        self.homeserver.is_reachable()
    }

    pub fn set_server_up(&self, up: bool) {
        self.homeserver.set_reachable(up);
        if !up {
            self.alice.client.set_connectivity(NetStatus::Offline);
            self.bob.client.set_connectivity(NetStatus::Offline);
        } else {
            for device in [&self.alice, &self.bob] {
                if device.matrix.is_connected() {
                    device.client.set_connectivity(NetStatus::Online);
                }
            }
        }
    }

    pub fn event_count(&self) -> usize {
        self.homeserver.event_count()
    }
}

fn build(
    me: &str,
    name: &str,
    homeserver: &FakeHomeserver,
    carrier: &FakeSmsNetwork,
    contacts: &[Contact],
) -> DemoDevice {
    let directory = MemoryDirectory::new(contacts.to_vec());
    let me = PeerId::new(me);
    let own = contacts
        .iter()
        .find(|c| c.peer == me)
        .expect("device knows itself");
    let matrix = homeserver.client(own.matrix_id.as_str());
    let sms = carrier.handset(own.phone.as_str());
    let clock = Arc::new(SystemClock);

    let client = Client::new(ClientParts {
        me: me.clone(),
        matrix: Arc::new(matrix.clone()),
        sms: Arc::new(sms.clone()),
        store: Arc::new(MemoryStore::new()),
        net: Arc::new(MemoryConnectivity::new(NetStatus::Online)),
        ids: Arc::new(EntropyIdGen::new(clock.clone())),
        clock,
        directory: Arc::new(directory),
    });

    DemoDevice {
        name: name.to_owned(),
        me,
        client,
        matrix,
        sms,
    }
}

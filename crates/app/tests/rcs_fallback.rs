mod harness;

use app::TransportBadge;
use domain::{NetStatus, PeerId};
use harness::{trailer_for, World};
use kit_arch::block_on;
use ports::Clock;

fn bob() -> PeerId {
    PeerId::new("bob")
}

#[test]
fn online_sends_go_over_matrix_and_never_touch_sms() {
    let world = World::new();

    block_on(world.alice.client.send(&bob(), "on my way"));
    block_on(world.bob.client.sync());

    assert_eq!(world.homeserver.event_count(), 1);
    assert!(
        world.bob.sms_storage().is_empty(),
        "a normal send must not cost an SMS"
    );

    let thread = world.bob.thread("alice");
    assert_eq!(thread.len(), 1);
    assert_eq!(thread[0].body, "on my way");
    assert_eq!(thread[0].badge, TransportBadge::Matrix);
    assert_eq!(world.alice.client.outbox_len(), 0);
}

#[test]
fn an_offline_send_falls_back_to_sms_carrying_its_id() {
    let world = World::new();
    world.alice.lose_data();

    let id = block_on(world.alice.client.send(&bob(), "running late"));

    assert_eq!(
        world.homeserver.event_count(),
        0,
        "nothing should have reached the homeserver"
    );

    let inbox = world.bob.sms_storage();
    assert_eq!(inbox.len(), 1);
    assert!(
        inbox[0].text.contains("running late"),
        "the human text must survive: {:?}",
        inbox[0].text
    );
    assert!(
        inbox[0].text.contains(&trailer_for(id)),
        "the id must ride along: {:?}",
        inbox[0].text
    );

    assert_eq!(world.alice.client.awaiting_backfill(), vec![id]);
    assert_eq!(
        world.alice.thread("bob")[0].badge,
        TransportBadge::Sms,
        "the sender's own view should say it went out over the fallback"
    );
}

#[test]
fn the_full_journey_leaves_the_recipient_with_one_message() {
    let world = World::new();

    world.alice.lose_data();
    let id = block_on(world.alice.client.send(&bob(), "dinner at eight?"));

    block_on(world.bob.client.sync());
    let thread = world.bob.thread("alice");
    assert_eq!(thread.len(), 1);
    assert_eq!(thread[0].badge, TransportBadge::Sms);
    assert_eq!(thread[0].body, "dinner at eight?", "trailer must be stripped");
    assert_eq!(
        world.bob.sms_storage().len(),
        1,
        "the SMS is really in Bob's SMS storage at this point"
    );

    world.clock.advance(120_000);
    world.alice.regain_data();
    let (_, pumped) = block_on(world.alice.client.sync());

    assert_eq!(pumped.backfilled, 1);
    assert_eq!(world.homeserver.event_count(), 1);
    assert_eq!(world.alice.client.outbox_len(), 0, "the entry is retired");

    let (ingested, _) = block_on(world.bob.client.sync());
    assert_eq!(ingested.superseded, 1);
    assert_eq!(ingested.inserted, 0);

    let thread = world.bob.thread("alice");
    assert_eq!(thread.len(), 1, "one message, not two");
    assert_eq!(world.bob.store.len(), 1, "and not filed under a second thread");
    assert_eq!(thread[0].badge, TransportBadge::Reconciled);
    assert_eq!(thread[0].body, "dinner at eight?");
    assert!(
        world.bob.sms_storage().is_empty(),
        "the SMS must be gone from device storage, not merely hidden"
    );

    assert!(
        thread[0].origin_ts < world.clock.now_ms(),
        "a backfilled message keeps its place in the thread"
    );
    assert_eq!(thread[0].tag, id.short_tag().to_base32());
}

#[test]
fn an_incoming_message_reconciles_on_the_receiving_side_too() {
    let world = World::new();

    world.bob.lose_data();
    block_on(world.bob.client.send(&PeerId::new("alice"), "sure, see you then"));
    block_on(world.alice.client.sync());

    let thread = world.alice.thread("bob");
    assert_eq!(thread.len(), 1);
    assert_eq!(thread[0].badge, TransportBadge::Sms);
    assert_eq!(thread[0].body, "sure, see you then");
    assert_eq!(
        world.alice.sms_storage().len(),
        1,
        "Alice really has the SMS at this point"
    );

    world.clock.advance(60_000);
    world.bob.regain_data();
    block_on(world.bob.client.sync());

    let (ingested, _) = block_on(world.alice.client.sync());
    assert_eq!(ingested.superseded, 1);

    let thread = world.alice.thread("bob");
    assert_eq!(thread.len(), 1, "one message, not two");
    assert_eq!(thread[0].badge, TransportBadge::Reconciled);
    assert!(
        world.alice.sms_storage().is_empty(),
        "Alice's SMS row must be gone from device storage"
    );
    assert_eq!(world.bob.client.outbox_len(), 0);
}

#[test]
fn a_matrix_event_that_overtakes_its_own_sms_still_dedupes() {
    let world = World::new();

    world.alice.lose_data();
    block_on(world.alice.client.send(&bob(), "call me"));
    world.clock.advance(60_000);
    world.alice.regain_data();
    block_on(world.alice.client.sync());

    let (ingested, _) = block_on(world.bob.client.sync());

    let thread = world.bob.thread("alice");
    assert_eq!(thread.len(), 1, "one message, not two");
    assert_eq!(thread[0].badge, TransportBadge::Reconciled);
    assert_eq!(ingested.superseded, 1);
    assert!(world.bob.sms_storage().is_empty());
}

#[test]
fn repeated_syncs_never_duplicate_anything() {
    let world = World::new();

    world.alice.lose_data();
    block_on(world.alice.client.send(&bob(), "still here"));
    block_on(world.bob.client.sync());
    world.clock.advance(60_000);
    world.alice.regain_data();

    for _ in 0..5 {
        block_on(world.alice.client.sync());
        block_on(world.bob.client.sync());
    }

    assert_eq!(world.homeserver.event_count(), 1);
    assert_eq!(world.bob.thread("alice").len(), 1);
    assert_eq!(world.alice.thread("bob").len(), 1);
    assert!(world.bob.sms_storage().is_empty());
}

#[test]
fn a_plain_sms_from_a_stranger_is_left_exactly_as_it_arrived() {
    let world = World::new();
    let stranger = world.stranger();

    block_on(ports::SmsTransport::send(
        &stranger,
        &world.bob.phone,
        "your parcel is out for delivery",
    ))
    .unwrap();
    block_on(world.bob.client.sync());

    let thread = world.bob.thread(harness::STRANGER_PHONE);
    assert_eq!(thread.len(), 1);
    assert_eq!(thread[0].body, "your parcel is out for delivery");
    assert_eq!(thread[0].badge, TransportBadge::Sms);
    assert_eq!(
        world.bob.sms_storage().len(),
        1,
        "there is no Matrix event coming, so the SMS must stay"
    );
}

#[test]
fn losing_all_service_marks_the_message_failed_rather_than_pretending() {
    let world = World::new();
    world.alice.lose_all_service();

    block_on(world.alice.client.send(&bob(), "anyone there?"));
    for _ in 0..domain::outbox::MAX_SMS_ATTEMPTS {
        block_on(world.alice.client.pump());
    }

    let thread = world.alice.thread("bob");
    assert_eq!(thread.len(), 1);
    assert_eq!(thread[0].badge, TransportBadge::Failed);
    assert_eq!(world.alice.client.outbox_len(), 0);
    assert!(world.bob.sms_storage().is_empty());
}

#[test]
fn a_server_outage_falls_back_even_though_the_device_has_data() {
    let world = World::new();
    world.homeserver.set_reachable(false);
    assert_eq!(world.alice.client.connectivity(), NetStatus::Online);

    let id = block_on(world.alice.client.send(&bob(), "server's down"));

    assert_eq!(world.homeserver.event_count(), 0);
    assert_eq!(world.alice.client.awaiting_backfill(), vec![id]);
    assert_eq!(world.bob.sms_storage().len(), 1);
    assert_eq!(
        world.alice.client.connectivity(),
        NetStatus::Offline,
        "a failed send is evidence, and should stop the next one trying"
    );
}

#[test]
fn a_recovered_server_backfills_everything_in_the_order_it_was_written() {
    let world = World::new();
    world.alice.lose_data();

    let ids: Vec<_> = ["first", "second", "third"]
        .into_iter()
        .map(|body| {
            world.clock.advance(1_000);
            block_on(world.alice.client.send(&bob(), body))
        })
        .collect();
    block_on(world.bob.client.sync());
    assert_eq!(world.bob.thread("alice").len(), 3);

    world.clock.advance(60_000);
    world.alice.regain_data();
    block_on(world.alice.client.sync());
    block_on(world.bob.client.sync());

    let thread = world.bob.thread("alice");
    assert_eq!(thread.len(), 3, "three messages, none duplicated");
    assert_eq!(
        thread.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
        vec!["first", "second", "third"],
        "compose order must survive the backfill"
    );
    for (view, id) in thread.iter().zip(&ids) {
        assert_eq!(view.badge, TransportBadge::Reconciled);
        assert_eq!(view.tag, id.short_tag().to_base32());
    }
    assert!(world.bob.sms_storage().is_empty());
}

#[test]
fn both_directions_work_while_one_side_is_offline() {
    let world = World::new();
    world.alice.lose_data();

    block_on(world.alice.client.send(&bob(), "you around?"));
    block_on(world.bob.client.sync());
    block_on(world.bob.client.send(&PeerId::new("alice"), "yes, here"));

    block_on(world.alice.client.sync());
    assert_eq!(world.alice.thread("bob").len(), 1);

    world.clock.advance(60_000);
    world.alice.regain_data();
    block_on(world.alice.client.sync());
    block_on(world.bob.client.sync());

    let alice_thread = world.alice.thread("bob");
    assert_eq!(alice_thread.len(), 2);
    assert_eq!(alice_thread[0].badge, TransportBadge::Reconciled);
    assert_eq!(alice_thread[1].body, "yes, here");
    assert_eq!(alice_thread[1].badge, TransportBadge::Matrix);

    assert_eq!(world.bob.thread("alice").len(), 2);
    assert!(world.bob.sms_storage().is_empty());
}

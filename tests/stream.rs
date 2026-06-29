//! Stream-service tests (Liquidsoap commands + realtime broadcast). Uses the
//! shared `ScenarioEnv` harness (see `common.rs`); each test supplies its own
//! Liquidsoap mock via `scenario_with_liquidsoap`.

mod common;
use common::*;

use caliborn::{RealtimeEvent, entities};
use sea_orm::{EntityTrait, PaginatorTrait};

#[tokio::test]
async fn record_played_inserts_history_increments_count_and_broadcasts() {
    let env = scenario_with_liquidsoap(MockLiquidsoapClient::new()).await;
    env.insert_song("/music/example.flac", "hash1", 180.0).await;

    let mut subscriber = env.broadcaster.subscribe();
    let stream = env.registry.stream_service();
    let played_at = stream
        .record_played(
            "/music/example.flac",
            Some("Title".into()),
            Some("Artist".into()),
            Some("Album".into()),
        )
        .await
        .unwrap();

    let history_count = entities::played_songs::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(history_count, 1);

    let song = entities::songs::Entity::find_by_id("/music/example.flac".to_string())
        .one(&*env.conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(song.played, 1);

    // Broadcaster should have received a NowPlaying event.
    let event = subscriber.recv().await.unwrap();
    match event {
        RealtimeEvent::NowPlaying {
            file_path,
            title,
            played_at: ev_played_at,
            ..
        } => {
            assert_eq!(file_path, "/music/example.flac");
            assert_eq!(title.as_deref(), Some("Title"));
            assert_eq!(ev_played_at, played_at);
        }
        other => panic!("expected NowPlaying, got {:?}", other),
    }
}

#[tokio::test]
async fn skip_invokes_liquidsoap_command() {
    let mut ls = MockLiquidsoapClient::new();
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "request.skip")
        .times(1)
        .returning(|_| Ok("Done.\nEND".to_string()));

    let env = scenario_with_liquidsoap(ls).await;
    let r = env.registry.stream_service().skip().await.unwrap();
    assert!(r.response.contains("Done"));
}

#[tokio::test]
async fn set_volume_rejects_out_of_range() {
    // Mock should NOT be called.
    let env = scenario_with_liquidsoap(MockLiquidsoapClient::new()).await;
    let err = env
        .registry
        .stream_service()
        .set_volume(2.0)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        caliborn::services::stream::StreamServiceError::InvalidVolume
    ));
}

#[tokio::test]
async fn push_queue_uses_correct_target() {
    let mut ls = MockLiquidsoapClient::new();
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "srq.push /music/song.flac")
        .times(1)
        .returning(|_| Ok("Done.\nEND".into()));
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "prioq.push /music/song.flac")
        .times(1)
        .returning(|_| Ok("Done.\nEND".into()));

    let env = scenario_with_liquidsoap(ls).await;
    let mut subscriber = env.broadcaster.subscribe();
    let stream = env.registry.stream_service();

    stream.push_queue("/music/song.flac", false).await.unwrap();
    stream.push_queue("/music/song.flac", true).await.unwrap();

    // Two QueueUpdated events should have been broadcast.
    for _ in 0..2 {
        match subscriber.recv().await.unwrap() {
            RealtimeEvent::QueueUpdated => {}
            other => panic!("expected QueueUpdated, got {:?}", other),
        }
    }
}

use std::sync::Arc;

use caliborn::{
    DiscordOAuthClient, RealtimeBroadcaster, RealtimeEvent, ServiceRegistry, build_oauth2_client,
    entities, liquidsoap::LiquidsoapClient, repositories::AlwaysCloneableConnection,
};
use hmac::{Hmac, Mac};
use migration::MigratorTrait;
use rstest::{fixture, rstest};
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait, PaginatorTrait};
use sha2::Sha256;
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::Mutex;

#[fixture]
async fn db() -> (AlwaysCloneableConnection, ContainerAsync<Postgres>) {
    let container = testcontainers_modules::postgres::Postgres::default()
        .with_tag("12")
        .start()
        .await
        .expect("Failed to start postgres container");

    let conn: DatabaseConnection = sea_orm::Database::connect(&format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    ))
    .await
    .expect("Failed to connect to postgres");

    migration::Migrator::up(&conn, None)
        .await
        .expect("Failed to run migrations");

    (AlwaysCloneableConnection::from(conn), container)
}

mockall::mock! {
    pub LiquidsoapClient {}

    #[async_trait::async_trait]
    impl LiquidsoapClient for LiquidsoapClient {
        async fn command(&mut self, cmd: &str) -> Result<String, caliborn::LiquidsoapError>;
        async fn command_with_reconnect(&mut self, cmd: &str) -> Result<String, caliborn::LiquidsoapError>;
        async fn shutdown(mut self) -> Result<(), caliborn::LiquidsoapError>;
    }
}

fn build(
    conn: AlwaysCloneableConnection,
    broadcaster: RealtimeBroadcaster,
    ls_mock: MockLiquidsoapClient,
) -> ServiceRegistry {
    let jwt = Hmac::<Sha256>::new_from_slice(b"jwt").unwrap();
    let hmac = Hmac::<Sha256>::new_from_slice(b"hmac").unwrap();
    let oauth: DiscordOAuthClient =
        build_oauth2_client("", "", "http://localhost:8080/callback").unwrap();
    let ls = Arc::new(Mutex::new(ls_mock)) as Arc<Mutex<dyn LiquidsoapClient>>;
    ServiceRegistry::new(
        conn,
        jwt,
        hmac,
        oauth,
        ls,
        broadcaster,
        "test_app_id".to_string(),
        "LumiRadio".to_string(),
    )
}

async fn insert_song(conn: &AlwaysCloneableConnection, file_path: &str, hash: &str) {
    entities::songs::Entity::insert(entities::songs::ActiveModel {
        file_path: ActiveValue::set(file_path.to_string()),
        title: ActiveValue::set("Title".into()),
        artist: ActiveValue::set("Artist".into()),
        album: ActiveValue::set("Album".into()),
        played: ActiveValue::set(0),
        requested: ActiveValue::set(0),
        duration: ActiveValue::set(180.0),
        file_hash: ActiveValue::set(hash.to_string()),
        bitrate: ActiveValue::set(320),
    })
    .exec(&**conn)
    .await
    .unwrap();
}

#[rstest]
#[awt]
#[tokio::test]
async fn record_played_inserts_history_increments_count_and_broadcasts(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_song(&conn, "/music/example.flac", "hash1").await;

    let broadcaster = RealtimeBroadcaster::new();
    let mut subscriber = broadcaster.subscribe();
    let registry = build(conn.clone(), broadcaster, MockLiquidsoapClient::new());

    let stream = registry.stream_service();
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
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(history_count, 1);

    let song = entities::songs::Entity::find_by_id("/music/example.flac".to_string())
        .one(&*conn)
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

#[rstest]
#[awt]
#[tokio::test]
async fn skip_invokes_liquidsoap_command(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;

    let mut ls = MockLiquidsoapClient::new();
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "request.skip")
        .times(1)
        .returning(|_| Ok("Done.\nEND".to_string()));

    let registry = build(conn.clone(), RealtimeBroadcaster::new(), ls);
    let r = registry.stream_service().skip().await.unwrap();
    assert!(r.response.contains("Done"));
}

#[rstest]
#[awt]
#[tokio::test]
async fn set_volume_rejects_out_of_range(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;

    // Mock should NOT be called.
    let ls = MockLiquidsoapClient::new();
    let registry = build(conn.clone(), RealtimeBroadcaster::new(), ls);
    let err = registry.stream_service().set_volume(2.0).await.unwrap_err();
    assert!(matches!(
        err,
        caliborn::services::stream::StreamServiceError::InvalidVolume
    ));
}

#[rstest]
#[awt]
#[tokio::test]
async fn push_queue_uses_correct_target(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;

    let mut ls = MockLiquidsoapClient::new();
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "srq.push /music/song.flac")
        .times(1)
        .returning(|_| Ok("Done.\nEND".into()));
    ls.expect_command_with_reconnect()
        .withf(|cmd| cmd == "prioq.push /music/song.flac")
        .times(1)
        .returning(|_| Ok("Done.\nEND".into()));

    let broadcaster = RealtimeBroadcaster::new();
    let mut subscriber = broadcaster.subscribe();
    let registry = build(conn.clone(), broadcaster, ls);
    let stream = registry.stream_service();

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

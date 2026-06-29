use std::sync::Arc;

use caliborn::{DiscordOAuthClient, build_oauth2_client, liquidsoap::LiquidsoapClient};
use hmac::{Hmac, Mac};
use migration::MigratorTrait;
use rstest::fixture;
use sea_orm::DatabaseConnection;
use sha2::Sha256;
use testcontainers::{
    ContainerAsync, ImageExt, core::logs::consumer::LogConsumer, runners::AsyncRunner,
};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::Mutex;

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use caliborn::{
    AppState, RealtimeBroadcaster, ServiceRegistry, entities,
    repositories::AlwaysCloneableConnection,
    services::{UserId, auth::Claims},
};
use sea_orm::{ActiveValue, EntityTrait};
use tower::ServiceExt;

async fn seed(conn: &DatabaseConnection) -> anyhow::Result<()> {
    caliborn::fixtures::seed::<caliborn::entities::songs::ActiveModel>(
        conn,
        "tests/fixtures/songs.yaml",
    )
    .await?;
    caliborn::fixtures::seed::<caliborn::entities::songs_fulltext::ActiveModel>(
        conn,
        "tests/fixtures/songs_fulltext.yaml",
    )
    .await?;

    Ok(())
}

#[allow(dead_code)]
struct StdoutLogConsumer;

impl LogConsumer for StdoutLogConsumer {
    fn accept<'a>(
        &'a self,
        record: &'a testcontainers::core::logs::LogFrame,
    ) -> serenity::futures::future::BoxFuture<'a, ()> {
        Box::pin(async move { println!("{}", String::from_utf8_lossy(record.bytes())) })
    }
}

#[fixture]
async fn db() -> (DatabaseConnection, ContainerAsync<Postgres>) {
    let container = testcontainers_modules::postgres::Postgres::default()
        .with_tag("12")
        .start()
        .await
        .expect("Failed to start postgres container");

    let conn = sea_orm::Database::connect(&format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    ))
    .await
    .expect("Failed to connect to postgres");

    migration::Migrator::up(&conn, None)
        .await
        .expect("Failed to run migrations");

    seed(&conn).await.expect("Failed to seed database");

    (conn, container)
}

#[fixture]
fn secret() -> Hmac<Sha256> {
    Hmac::new_from_slice(b"secret").expect("Failed to create HMAC")
}

#[fixture]
fn hmac_secret() -> Hmac<Sha256> {
    Hmac::new_from_slice(b"hmac_secret").expect("Failed to create HMAC")
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

#[fixture]
fn liquidsoap() -> Arc<Mutex<dyn LiquidsoapClient>> {
    let mock = MockLiquidsoapClient::new();
    Arc::new(Mutex::new(mock))
}

#[fixture]
fn discord_client() -> DiscordOAuthClient {
    build_oauth2_client("", "", "http://localhost:8080/callback").unwrap()
}

#[fixture]
pub async fn app(
    #[future] db: (DatabaseConnection, ContainerAsync<Postgres>),
    secret: Hmac<Sha256>,
    hmac_secret: Hmac<Sha256>,
    liquidsoap: Arc<Mutex<dyn LiquidsoapClient>>,
    discord_client: DiscordOAuthClient,
) -> (axum::Router, ContainerAsync<Postgres>) {
    let (db, container) = db.await;
    (
        caliborn::make_app(
            secret,
            hmac_secret,
            discord_client,
            db.into(),
            liquidsoap,
            "test_app_id".to_string(),
            "LumiRadio".to_string(),
            std::sync::Arc::new(
                caliborn::services::secrets::TokenSealer::from_hex(
                    "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
                )
                .unwrap(),
            ),
            std::sync::Arc::<str>::from("test_liquidsoap_token"),
            "playlist".to_string(),
        ),
        container,
    )
}

#[allow(dead_code)]
pub fn with_tracing<T>(f: impl FnOnce() -> T) -> T {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_line_number(true)
        .with_file(true)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish();

    tracing::subscriber::with_default(subscriber, f)
}

// ---------------------------------------------------------------------------
// Cross-service scenario harness
// ---------------------------------------------------------------------------
//
// Unlike the per-service fixtures above (each test file builds its own
// registry), `ScenarioEnv` wires ONE `ServiceRegistry` + `Broadcaster` +
// Liquidsoap mock over a single testcontainer DB, and builds an `axum::Router`
// that shares that exact state (via `caliborn::router`). This lets a single
// test drive a full user journey across several services either at the service
// layer (`env.registry`) or over HTTP (`env.request`), and assert the same DB.

fn jwt_key() -> Hmac<Sha256> {
    Hmac::new_from_slice(b"jwt").expect("Failed to create JWT HMAC")
}

fn scenario_sealer() -> std::sync::Arc<caliborn::services::secrets::TokenSealer> {
    std::sync::Arc::new(
        caliborn::services::secrets::TokenSealer::from_hex(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        )
        .unwrap(),
    )
}

/// A Liquidsoap mock that accepts any command — for scenarios that don't care
/// about the exact commands sent.
#[allow(dead_code)]
pub fn permissive_liquidsoap() -> MockLiquidsoapClient {
    let mut mock = MockLiquidsoapClient::new();
    mock.expect_command_with_reconnect()
        .returning(|_| Ok("Done.\nEND".to_string()));
    mock.expect_command().returning(|_| Ok("[]".to_string()));
    mock
}

/// Spin up a fresh testcontainer Postgres, migrate, and seed the song fixtures.
async fn scenario_conn() -> (AlwaysCloneableConnection, ContainerAsync<Postgres>) {
    let container = Postgres::default()
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

    seed(&conn).await.expect("Failed to seed database");

    (AlwaysCloneableConnection::from(conn), container)
}

async fn build_scenario(mock: MockLiquidsoapClient) -> ScenarioEnv {
    let (conn, container) = scenario_conn().await;

    let jwt = jwt_key();
    let hmac = Hmac::<Sha256>::new_from_slice(b"hmac").unwrap();
    let oauth = build_oauth2_client("", "", "http://localhost:8080/callback").unwrap();
    let ls = Arc::new(Mutex::new(mock)) as Arc<Mutex<dyn LiquidsoapClient>>;
    let broadcaster = RealtimeBroadcaster::new();

    let registry = ServiceRegistry::new(
        conn.clone(),
        jwt.clone(),
        hmac,
        oauth,
        ls,
        broadcaster.clone(),
        "test_app_id".to_string(),
        "LumiRadio".to_string(),
        scenario_sealer(),
        "playlist".to_string(),
    );

    let app_state = AppState {
        service_registry: registry.clone(),
        liquidsoap_ingest_token: std::sync::Arc::<str>::from("test_liquidsoap_token"),
    };
    let router = caliborn::router(app_state);

    ScenarioEnv {
        registry,
        router,
        conn,
        broadcaster,
        jwt_secret: jwt,
        _container: container,
    }
}

/// Default scenario harness with a permissive Liquidsoap mock.
#[fixture]
#[allow(dead_code)]
pub async fn scenario() -> ScenarioEnv {
    build_scenario(permissive_liquidsoap()).await
}

/// Scenario harness with a caller-configured Liquidsoap mock (for asserting the
/// exact commands sent, e.g. `srq.push <path>`).
#[allow(dead_code)]
pub async fn scenario_with_liquidsoap(mock: MockLiquidsoapClient) -> ScenarioEnv {
    build_scenario(mock).await
}

#[allow(dead_code)]
pub struct ScenarioEnv {
    /// Drive services directly (service-layer scenarios).
    pub registry: ServiceRegistry,
    /// Drive the real router over HTTP (shares `registry`'s state).
    pub router: Router,
    /// Raw DB handle for assertions.
    pub conn: AlwaysCloneableConnection,
    /// Subscribe before an action to assert broadcast events.
    pub broadcaster: RealtimeBroadcaster,
    jwt_secret: Hmac<Sha256>,
    _container: ContainerAsync<Postgres>,
}

#[allow(dead_code)]
impl ScenarioEnv {
    pub async fn insert_user(&self, id: i64, boonbucks: i32, watched_time: i64) {
        entities::users::Entity::insert(entities::users::ActiveModel {
            id: ActiveValue::set(id),
            boonbucks: ActiveValue::set(boonbucks),
            watched_time: ActiveValue::set(watched_time),
            ..Default::default()
        })
        .exec(&*self.conn)
        .await
        .unwrap();
    }

    pub async fn insert_song(&self, file_path: &str, file_hash: &str, duration: f64) {
        entities::songs::Entity::insert(entities::songs::ActiveModel {
            file_path: ActiveValue::set(file_path.to_string()),
            title: ActiveValue::set("Scenario".into()),
            artist: ActiveValue::set("Scenario".into()),
            album: ActiveValue::set("Scenario".into()),
            played: ActiveValue::set(0),
            requested: ActiveValue::set(0),
            duration: ActiveValue::set(duration),
            file_hash: ActiveValue::set(file_hash.to_string()),
            bitrate: ActiveValue::set(320),
        })
        .exec(&*self.conn)
        .await
        .unwrap();
    }

    /// Insert a user with the full set of fields the SLCB/migration flows care
    /// about (notably `migrated`).
    pub async fn insert_user_full(
        &self,
        id: i64,
        boonbucks: i32,
        watched_time: i64,
        migrated: bool,
    ) {
        entities::users::Entity::insert(entities::users::ActiveModel {
            id: ActiveValue::set(id),
            boonbucks: ActiveValue::set(boonbucks),
            watched_time: ActiveValue::set(watched_time),
            migrated: ActiveValue::set(migrated),
            ..Default::default()
        })
        .exec(&*self.conn)
        .await
        .unwrap();
    }

    /// Link a YouTube channel to a user (for SLCB-match / connections flows).
    pub async fn link_youtube(&self, user_id: i64, channel_id: &str) {
        entities::connected_youtube_accounts::Entity::insert(
            entities::connected_youtube_accounts::ActiveModel {
                user_id: ActiveValue::set(user_id),
                youtube_channel_id: ActiveValue::set(channel_id.to_string()),
                youtube_channel_name: ActiveValue::set("Channel".into()),
                ..Default::default()
            },
        )
        .exec(&*self.conn)
        .await
        .unwrap();
    }

    pub async fn balance(&self, id: i64) -> i32 {
        entities::users::Entity::find_by_id(id)
            .one(&*self.conn)
            .await
            .unwrap()
            .unwrap()
            .boonbucks
    }

    /// Mint a JWT the router's `authenticate` middleware will accept for `id`.
    pub fn mint_jwt(&self, id: i64) -> String {
        Claims::new(UserId::from(id), chrono::Duration::hours(1))
            .sign(&self.jwt_secret)
            .unwrap()
    }

    /// Fire one HTTP request at the router and return `(status, json_body)`.
    /// `json_body` is `Value::Null` when the response has no/invalid JSON body.
    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        jwt: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(jwt) = jwt {
            builder = builder.header("authorization", format!("Bearer {jwt}"));
        }
        let req = match body {
            Some(b) => builder
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&b).unwrap()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };

        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, json)
    }
}

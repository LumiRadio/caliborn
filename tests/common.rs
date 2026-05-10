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

async fn seed(conn: &DatabaseConnection) -> anyhow::Result<()> {
    caliborn::fixtures::seed::<caliborn::entities::songs::ActiveModel>(
        &conn,
        "tests/fixtures/songs.yaml",
    )
    .await?;
    caliborn::fixtures::seed::<caliborn::entities::songs_fulltext::ActiveModel>(
        &conn,
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
        Box::pin(async move { println!("{}", String::from_utf8_lossy(&record.bytes())) })
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

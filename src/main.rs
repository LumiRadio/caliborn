use std::{path::PathBuf, sync::Arc};

use caliborn::{LiquidsoapClientImpl, LiquidsoapError};
use clap::{Parser, Subcommand};
use hmac::{Hmac, Mac};
use migration::MigratorTrait;
use sha2::Sha256;
use tokio::sync::Mutex;

use crate::config::Config;

mod config;

#[derive(thiserror::Error, Debug)]
enum ApplicationError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Invalid configuration: {0}")]
    Figment(#[from] figment::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Hmac(#[from] hmac::digest::InvalidLength),
    #[error(transparent)]
    SeaOrm(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Liquidsoap(#[from] LiquidsoapError),
    #[error("{0}")]
    NotImplemented(&'static str),
    #[error("Linked-roles error: {0}")]
    LinkedRoles(String),
    #[error(transparent)]
    Sealer(#[from] caliborn::services::secrets::SealerError),
}

#[derive(Parser)]
#[command(
    name = "caliborn",
    about = "LumiRadio backend server and operations CLI",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server (default when no subcommand is given).
    Serve,

    /// Database migration operations.
    Migrate {
        #[command(subcommand)]
        op: MigrateOp,
    },

    /// Index a music directory into the database.
    Index {
        /// Path to the music root.
        path: PathBuf,
        /// Optional path to also write a refreshed `.m3u` playlist to.
        #[arg(short, long)]
        playlist: Option<PathBuf>,
        /// Don't write to the database; report what would change.
        #[arg(long)]
        dry_run: bool,
    },

    /// Drop song rows whose files no longer exist on disk and prune orphans.
    Housekeep {
        /// Path to the music root.
        path: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },

    /// (Re)write the `.m3u` playlist from current `songs` rows.
    Playlist {
        /// Output path for the `.m3u` file.
        out: PathBuf,
        /// After writing, ask Liquidsoap to reload it.
        #[arg(long)]
        reload: bool,
    },

    /// Discord linked-roles management.
    LinkedRoles {
        #[command(subcommand)]
        op: LinkedRolesOp,
    },

    /// Import legacy Streamlabs Chatbot data from a JSON dump.
    ImportSlcb {
        path: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },

    /// Re-run the YouTube-channel-id matching pass against `connected_youtube_accounts`.
    MatchSlcb,
}

#[derive(Subcommand)]
enum MigrateOp {
    /// Apply all pending migrations.
    Up {
        /// Apply at most N migrations.
        #[arg(short, long)]
        steps: Option<u32>,
    },
    /// Roll back the last applied migration.
    Down {
        #[arg(short, long, default_value_t = 1)]
        steps: u32,
    },
    /// Print migration status.
    Status,
    /// Drop everything and re-apply (DANGEROUS — dev only).
    Fresh,
}

#[derive(Subcommand)]
enum LinkedRolesOp {
    /// Register the role-connections metadata schema with Discord.
    /// Requires a bot token via `--bot-token` or `CALIBORN_DISCORD_BOT_TOKEN`.
    Register {
        /// Discord bot token. Falls back to `CALIBORN_DISCORD_BOT_TOKEN` env var.
        #[arg(long)]
        bot_token: Option<String>,
    },
}

async fn serve(config: Config) -> Result<(), ApplicationError> {
    let oauth_client = caliborn::build_oauth2_client(
        &config.discord.client_id,
        &config.discord.client_secret,
        &config.discord.redirect_uri,
    )
    .inspect_err(|e| tracing::error!(error = ?e))?;
    let secret: Hmac<Sha256> = Hmac::new_from_slice(config.jwt.secret.as_bytes())
        .inspect_err(|e| tracing::error!(error = ?e))?;
    let hmac_secret: Hmac<Sha256> = Hmac::new_from_slice(config.bot_auth.secret_key.as_bytes())
        .inspect_err(|e| tracing::error!(error = ?e))?;
    let db = sea_orm::Database::connect(&config.database_url)
        .await
        .inspect_err(|e| tracing::error!(error = ?e))?;
    let liquidsoap_client = LiquidsoapClientImpl::new(&config.liquidsoap_socket)
        .await
        .inspect_err(|e| tracing::error!(error = ?e))?;

    let token_sealer = Arc::new(
        caliborn::services::secrets::TokenSealer::from_env()
            .inspect_err(|e| tracing::error!(error = ?e))?,
    );

    let app = caliborn::make_app(
        secret,
        hmac_secret,
        oauth_client,
        db.into(),
        Arc::new(Mutex::new(liquidsoap_client)),
        config.discord.client_id.clone(),
        "LumiRadio".to_string(),
        token_sealer,
    );
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000")
        .await
        .inspect_err(|e| tracing::error!(error = ?e))?;

    axum::serve(listener, app)
        .await
        .inspect_err(|e| tracing::error!(error = ?e))?;

    Ok(())
}

async fn linked_roles(config: Config, op: LinkedRolesOp) -> Result<(), ApplicationError> {
    use caliborn::services::discord_linked_roles::{LinkedRolesService, default_schema};

    match op {
        LinkedRolesOp::Register { bot_token } => {
            let bot_token = bot_token
                .or_else(|| std::env::var("CALIBORN_DISCORD_BOT_TOKEN").ok())
                .ok_or(ApplicationError::NotImplemented(
                    "Provide --bot-token or set CALIBORN_DISCORD_BOT_TOKEN",
                ))?;

            // We don't need a DB for register; pass an unused connection.
            let db = sea_orm::Database::connect(&config.database_url).await?;
            let http_client = reqwest::Client::new();
            let service = LinkedRolesService::new(
                http_client,
                config.discord.client_id.clone(),
                "LumiRadio".to_string(),
                db.into(),
            );
            service
                .register_metadata(&bot_token, &default_schema())
                .await
                .map_err(|e| ApplicationError::LinkedRoles(e.to_string()))?;
            tracing::info!("Linked-roles metadata schema registered.");
            Ok(())
        }
    }
}

async fn import_slcb(config: Config, path: PathBuf, dry_run: bool) -> Result<(), ApplicationError> {
    use caliborn::services::slcb;

    let records = slcb::parse_streamlabs(&path)
        .map_err(|e| ApplicationError::LinkedRoles(format!("import-slcb parse error: {e}")))?;
    let db = sea_orm::Database::connect(&config.database_url).await?;
    let conn: caliborn::repositories::AlwaysCloneableConnection = db.into();
    let summary = slcb::import_records(&conn, &records, dry_run)
        .await
        .map_err(|e| ApplicationError::LinkedRoles(format!("import-slcb failed: {e}")))?;
    tracing::info!(
        inserted = summary.inserted,
        updated = summary.updated,
        skipped = summary.skipped,
        dry_run,
        "SLCB import complete"
    );
    println!(
        "SLCB import: inserted={} updated={} skipped={} (dry_run={})",
        summary.inserted, summary.updated, summary.skipped, dry_run
    );
    Ok(())
}

async fn match_slcb(config: Config) -> Result<(), ApplicationError> {
    use caliborn::services::slcb;

    let db = sea_orm::Database::connect(&config.database_url).await?;
    let conn: caliborn::repositories::AlwaysCloneableConnection = db.into();
    let summary = slcb::match_youtube_links(&conn)
        .await
        .map_err(|e| ApplicationError::LinkedRoles(format!("match-slcb failed: {e}")))?;
    tracing::info!(
        considered = summary.considered,
        matched = summary.matched,
        already_migrated = summary.already_migrated,
        no_slcb_row = summary.no_slcb_row,
        "SLCB match complete"
    );
    println!(
        "SLCB match: considered={} matched={} already_migrated={} no_slcb_row={}",
        summary.considered, summary.matched, summary.already_migrated, summary.no_slcb_row
    );
    Ok(())
}

async fn migrate(config: Config, op: MigrateOp) -> Result<(), ApplicationError> {
    let db = sea_orm::Database::connect(&config.database_url).await?;
    match op {
        MigrateOp::Up { steps } => migration::Migrator::up(&db, steps).await?,
        MigrateOp::Down { steps } => migration::Migrator::down(&db, Some(steps)).await?,
        MigrateOp::Status => migration::Migrator::status(&db).await?,
        MigrateOp::Fresh => migration::Migrator::fresh(&db).await?,
    }
    Ok(())
}

async fn dispatch(cli: Cli) -> Result<(), ApplicationError> {
    let config = Config::new()?;
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(config).await,
        Command::Migrate { op } => migrate(config, op).await,
        Command::Index { .. } => Err(ApplicationError::NotImplemented(
            "`index`: port from frohike not yet landed",
        )),
        Command::Housekeep { .. } => Err(ApplicationError::NotImplemented(
            "`housekeep`: port from frohike not yet landed",
        )),
        Command::Playlist { .. } => Err(ApplicationError::NotImplemented(
            "`playlist`: port from frohike not yet landed",
        )),
        Command::LinkedRoles { op } => linked_roles(config, op).await,
        Command::ImportSlcb { path, dry_run } => import_slcb(config, path, dry_run).await,
        Command::MatchSlcb => match_slcb(config).await,
    }
}

fn main() -> Result<(), ApplicationError> {
    let cli = Cli::parse();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(dispatch(cli))
}

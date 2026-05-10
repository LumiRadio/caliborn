use std::path::{Path, PathBuf};

use sea_orm::{
    ActiveValue, ConnectionTrait, EntityTrait, QueryFilter, prelude::*, sea_query::OnConflict,
};
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, warn};

use crate::entities;
use crate::maintenance::{
    MaintenanceError, is_supported_audio, metadata::MusicMetadata, rewrite_music_path,
};

#[tracing::instrument(skip(db))]
pub async fn index<C: ConnectionTrait>(
    db: &C,
    directory: PathBuf,
    dry_run: bool,
) -> Result<(), MaintenanceError> {
    if dry_run {
        info!(
            "dry_run: would prune indexing tables and re-index {}",
            directory.display()
        );
    } else {
        info!("Pruning indexing database");
        prune_all(db).await?;
    }

    let files: Vec<PathBuf> = walkdir::WalkDir::new(&directory)
        .into_iter()
        .filter_map(|e| match e {
            Ok(entry) => Some(entry),
            Err(err) => {
                error!("Failed to walk directory: {}", err);
                None
            }
        })
        .filter(|e| e.file_type().is_file() && is_supported_audio(e.path()))
        .map(|e| e.path().to_owned())
        .collect();

    debug!("Found {} files", files.len());

    let len = files.len();
    let mut failed_files = Vec::new();
    for file in files {
        if let Err(e) = index_file(db, &file, &directory, dry_run).await {
            error!("failed to index file {}: {}", file.display(), e);
            failed_files.push(file);
        }
    }
    info!("Indexed {} files", len);
    if !failed_files.is_empty() {
        warn!("Failed to index {} files", failed_files.len());
        warn!("Failed files: {:#?}", failed_files);
    }

    Ok(())
}

#[tracing::instrument(skip(db))]
pub async fn index_file<C: ConnectionTrait>(
    db: &C,
    path: &Path,
    music_path: &Path,
    dry_run: bool,
) -> Result<(), MaintenanceError> {
    if !is_supported_audio(path) {
        return Ok(());
    }

    let meta = MusicMetadata::new(path)?;

    let mut hasher: Sha256 = Digest::new();
    hasher.update(path.canonicalize()?.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    let hash_str = format!("{:x}", hash);

    let rewritten = rewrite_music_path(path, music_path)?;
    let title = meta.title.replace(char::from(0), "");
    let artist = meta.artist.replace(char::from(0), "");
    let album = meta.album.replace(char::from(0), "");
    let file_path = rewritten.display().to_string();

    info!(
        "Indexing {title} by {artist} on {album} at path {file_path}",
        title = title,
        artist = artist,
        album = album,
        file_path = file_path,
    );

    if dry_run {
        debug!("dry_run: skipping write for {}", file_path);
        return Ok(());
    }

    upsert_song(
        db,
        &file_path,
        &hash_str,
        &title,
        &artist,
        &album,
        meta.duration,
        meta.bitrate as i32,
    )
    .await?;

    upsert_song_fulltext(db, &hash_str, &title, &artist, &album).await?;

    replace_song_tags(db, &hash_str, &meta.tags).await?;

    Ok(())
}

pub async fn drop_index<C: ConnectionTrait>(
    db: &C,
    path: &Path,
    music_path: &Path,
) -> Result<(), MaintenanceError> {
    let db_path = rewrite_music_path(path, music_path)?;
    let db_path_str = db_path.display().to_string();
    info!("Dropping index for {}", db_path_str);

    let songs = entities::songs::Entity::find()
        .filter(entities::songs::Column::FilePath.eq(&db_path_str))
        .all(db)
        .await?;

    delete_songs(db, &songs).await
}

pub async fn drop_index_folder<C: ConnectionTrait>(
    db: &C,
    folder_path: &Path,
    music_path: &Path,
) -> Result<(), MaintenanceError> {
    let db_path = rewrite_music_path(folder_path, music_path)?;
    let prefix = format!("{}%", db_path.display());
    info!("Dropping index for folder {}", db_path.display());

    let songs = entities::songs::Entity::find()
        .filter(entities::songs::Column::FilePath.like(&prefix))
        .all(db)
        .await?;

    delete_songs(db, &songs).await
}

async fn delete_songs<C: ConnectionTrait>(
    db: &C,
    songs: &[entities::songs::Model],
) -> Result<(), MaintenanceError> {
    if songs.is_empty() {
        return Ok(());
    }

    let hashes: Vec<String> = songs.iter().map(|s| s.file_hash.clone()).collect();

    entities::song_tags::Entity::delete_many()
        .filter(entities::song_tags::Column::SongId.is_in(hashes.clone()))
        .exec(db)
        .await?;

    entities::songs_fulltext::Entity::delete_many()
        .filter(entities::songs_fulltext::Column::SongId.is_in(hashes.clone()))
        .exec(db)
        .await?;

    entities::songs::Entity::delete_many()
        .filter(entities::songs::Column::FileHash.is_in(hashes))
        .exec(db)
        .await?;

    Ok(())
}

async fn prune_all<C: ConnectionTrait>(db: &C) -> Result<(), MaintenanceError> {
    entities::song_tags::Entity::delete_many().exec(db).await?;
    entities::songs_fulltext::Entity::delete_many()
        .exec(db)
        .await?;
    entities::songs::Entity::delete_many().exec(db).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_song<C: ConnectionTrait>(
    db: &C,
    file_path: &str,
    file_hash: &str,
    title: &str,
    artist: &str,
    album: &str,
    duration: f64,
    bitrate: i32,
) -> Result<(), MaintenanceError> {
    let model = entities::songs::ActiveModel {
        file_path: ActiveValue::set(file_path.to_string()),
        title: ActiveValue::set(title.to_string()),
        artist: ActiveValue::set(artist.to_string()),
        album: ActiveValue::set(album.to_string()),
        duration: ActiveValue::set(duration),
        file_hash: ActiveValue::set(file_hash.to_string()),
        bitrate: ActiveValue::set(bitrate),
        played: ActiveValue::not_set(),
        requested: ActiveValue::not_set(),
    };

    entities::songs::Entity::insert(model)
        .on_conflict(
            OnConflict::column(entities::songs::Column::FilePath)
                .update_columns([
                    entities::songs::Column::Title,
                    entities::songs::Column::Artist,
                    entities::songs::Column::Album,
                    entities::songs::Column::Duration,
                    entities::songs::Column::FileHash,
                    entities::songs::Column::Bitrate,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(())
}

async fn upsert_song_fulltext<C: ConnectionTrait>(
    db: &C,
    file_hash: &str,
    title: &str,
    artist: &str,
    album: &str,
) -> Result<(), MaintenanceError> {
    let model = entities::songs_fulltext::ActiveModel {
        song_id: ActiveValue::set(file_hash.to_string()),
        title: ActiveValue::set(title.to_string()),
        artist: ActiveValue::set(artist.to_string()),
        album: ActiveValue::set(album.to_string()),
        tsvector: ActiveValue::not_set(),
    };

    entities::songs_fulltext::Entity::insert(model)
        .on_conflict(
            OnConflict::column(entities::songs_fulltext::Column::SongId)
                .update_columns([
                    entities::songs_fulltext::Column::Title,
                    entities::songs_fulltext::Column::Artist,
                    entities::songs_fulltext::Column::Album,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(())
}

async fn replace_song_tags<C: ConnectionTrait>(
    db: &C,
    file_hash: &str,
    tags: &[(String, String)],
) -> Result<(), MaintenanceError> {
    entities::song_tags::Entity::delete_many()
        .filter(entities::song_tags::Column::SongId.eq(file_hash))
        .exec(db)
        .await?;

    if tags.is_empty() {
        return Ok(());
    }

    let to_insert: Vec<entities::song_tags::ActiveModel> = tags
        .iter()
        .map(|(k, v)| entities::song_tags::ActiveModel {
            song_id: ActiveValue::set(file_hash.to_string()),
            tag: ActiveValue::set(k.clone()),
            value: ActiveValue::set(v.clone()),
            ..Default::default()
        })
        .collect();

    entities::song_tags::Entity::insert_many(to_insert)
        .exec(db)
        .await?;

    Ok(())
}

use std::path::Path;

use sea_orm::{ConnectionTrait, EntityTrait, FromQueryResult, QuerySelect};

use crate::entities;
use crate::maintenance::MaintenanceError;

#[derive(FromQueryResult)]
struct PathOnly {
    file_path: String,
}

pub async fn create_playlist<C: ConnectionTrait>(
    db: &C,
    playlist_path: &Path,
) -> Result<(), MaintenanceError> {
    let paths: Vec<String> = entities::songs::Entity::find()
        .select_only()
        .column(entities::songs::Column::FilePath)
        .into_model::<PathOnly>()
        .all(db)
        .await?
        .into_iter()
        .map(|p| p.file_path)
        .collect();

    let mut file = std::fs::File::create(playlist_path)?;
    let mut writer = m3u::Writer::new(&mut file);
    for entry in paths.into_iter().map(m3u::path_entry) {
        writer.write_entry(&entry).map_err(std::io::Error::from)?;
    }

    Ok(())
}

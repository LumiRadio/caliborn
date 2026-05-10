use std::path::{Path, PathBuf};

pub mod indexing;
pub mod metadata;
pub mod playlist;

pub static SUPPORTED_AUDIO_FORMATS: [&str; 4] = ["mp3", "flac", "ogg", "wav"];

#[derive(thiserror::Error, Debug)]
pub enum MaintenanceError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    StripPrefix(#[from] std::path::StripPrefixError),
    #[error(transparent)]
    SeaOrm(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Lofty(#[from] lofty::error::LoftyError),
    #[error(transparent)]
    Notify(#[from] notify::Error),
}

pub fn rewrite_music_path(path: &Path, music_path: &Path) -> Result<PathBuf, MaintenanceError> {
    Ok(Path::new("/music").join(path.strip_prefix(music_path)?))
}

pub fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .map(|ext| SUPPORTED_AUDIO_FORMATS.contains(&ext.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

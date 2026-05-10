use std::path::Path;

use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::ItemValue;

pub struct MusicMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    /// Bitrate in bits per second (lofty reports kbps; we scale to match
    /// the prior ffmpeg-derived shape stored in `songs.bitrate`).
    pub bitrate: i64,
    pub tags: Vec<(String, String)>,
}

impl MusicMetadata {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, lofty::error::LoftyError> {
        let tagged = Probe::open(path)?.guess_file_type()?.read()?;
        let props = tagged.properties();

        let duration = props.duration().as_secs_f64();
        let bitrate = props
            .audio_bitrate()
            .map(|kbps| i64::from(kbps) * 1000)
            .unwrap_or(0);

        let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

        let (title, artist, album) = match tag {
            Some(t) => (
                t.title().as_deref().unwrap_or("").to_owned(),
                t.artist().as_deref().unwrap_or("").to_owned(),
                t.album().as_deref().unwrap_or("").to_owned(),
            ),
            None => (String::new(), String::new(), String::new()),
        };

        let tags = match tag {
            Some(t) => {
                let tag_type = t.tag_type();
                t.items()
                    .filter_map(|item| {
                        let ItemValue::Text(value) = item.value() else {
                            return None;
                        };
                        if value.is_empty() {
                            return None;
                        }
                        let key = item.key().map_key(tag_type)?;
                        Some((key.to_string(), value.clone()))
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        Ok(Self {
            title,
            artist,
            album,
            duration,
            bitrate,
            tags,
        })
    }
}

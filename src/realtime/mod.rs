//! In-process pub/sub for runtime events.
//!
//! Events are published by the playback ingest endpoint, the song request
//! flow, and minigame services. Subscribers (currently the WebSocket route)
//! receive a fan-out copy of every event published while their receiver is
//! alive. Lagged subscribers drop the oldest events.

use chrono::NaiveDateTime;
use serde::Serialize;
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 256;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    NowPlaying {
        file_path: String,
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        played_at: NaiveDateTime,
    },
    QueueUpdated,
}

#[derive(Clone)]
pub struct Broadcaster {
    sender: broadcast::Sender<Event>,
}

impl Broadcaster {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Best-effort publish. Errors when there are no subscribers — that's not
    /// a failure for callers, so the result is intentionally discarded.
    pub fn send(&self, event: Event) {
        let _ = self.sender.send(event);
    }
}

impl Default for Broadcaster {
    fn default() -> Self {
        Self::new()
    }
}

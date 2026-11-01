//! Tracker status events matching `ziyron_core::StatusEvent` tracker variants.
//!
//! Defined here so `ziyron-discovery` does not depend on `ziyron-core`. Core maps these
//! into its own `StatusEvent` enum when wiring callbacks.

use ziyron_types::InfoHash;

/// Tracker lifecycle / announce outcomes (Go `StatusUpdatedEvent` tracker kinds).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackerStatusEvent {
    Connected {
        url: String,
        info_hash: InfoHash,
    },
    Disconnected {
        url: String,
        info_hash: InfoHash,
        error: Option<String>,
    },
    AnnounceSuccessful {
        url: String,
        info_hash: InfoHash,
    },
    AnnounceError {
        url: String,
        info_hash: InfoHash,
        error: String,
    },
}

/// Callback sink for announcer / WebTorrent tracker status.
pub type StatusCallback = std::sync::Arc<dyn Fn(&TrackerStatusEvent) + Send + Sync>;

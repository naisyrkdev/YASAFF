
pub mod burn;
pub mod ids;
pub mod manifest;
pub mod resolve;
pub mod watch;

pub use ids::ExternalId;
pub use manifest::DiscManifest;
pub use resolve::{GameIds, Resolution, resolve};
pub use watch::{DiscEvent, Volume, VolumeKind, WatchOptions, Watcher, removable_volumes};

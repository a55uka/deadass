mod poller;
mod sender;

pub use poller::{ScoreboardDelta, ScoreboardSnapshot};
pub use sender::EventSender;

#[cfg(windows)]
mod entry;

#[cfg(windows)]
pub use entry::DllEntry;

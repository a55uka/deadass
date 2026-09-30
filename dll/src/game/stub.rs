use crate::diff::Snapshot;
use crate::offsets::Offsets;

#[derive(Debug, Default)]
pub struct Reader;

impl Reader {
    pub fn new() -> Self {
        Self
    }

    pub fn snapshot(&mut self, _offsets: &Offsets) -> Snapshot {
        Snapshot::default()
    }
}

pub fn client_base() -> u64 {
    0
}

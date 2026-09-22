use crate::diff::Snapshot;

pub(super) fn snapshot(_offsets: &crate::offsets::Offsets) -> Snapshot {
    Snapshot::default()
}

pub(super) fn client_base() -> u64 {
    0
}

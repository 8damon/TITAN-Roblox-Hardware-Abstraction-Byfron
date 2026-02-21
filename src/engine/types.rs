#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionMode {
    OneShot,
    BackgroundSilent,
    BackgroundNotify,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundState {
    Armed,
    IgnoreNextClose { session_seen: bool },
    WaitForRealPlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessProvenance {
    InstallerSpawned,
    UserSpawned,
}

pub(crate) struct PlayerTrack {
    pub started: std::time::Instant,
    pub generation: u64,
    pub saw_window: bool,
    pub provenance: ProcessProvenance,
}

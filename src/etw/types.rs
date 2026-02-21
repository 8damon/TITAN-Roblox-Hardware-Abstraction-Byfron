#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RobloxExe {
    RobloxPlayerBeta,
    RobloxPlayerLauncher,
    RobloxCrashHandler,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RobloxInstance {
    pub exe: RobloxExe,
    pub pid: u32,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RobloxAlert {
    ProcessStart { instance: RobloxInstance },
    ProcessStop { instance: RobloxInstance },
    SWait,
    SReady,
}

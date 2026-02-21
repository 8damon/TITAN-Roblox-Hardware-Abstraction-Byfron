pub mod bootstrapper;
pub mod web;

use crate::setup::setup::ArConfig;

pub enum InstallLaunch {
    Bootstrapper,
    RobloxInstaller,
    None,
}

pub fn ArInstall(cfg: &ArConfig) -> InstallLaunch {
    let use_bootstrapper = cfg.bootstrapper.use_bootstrapper;
    let prefer_bootstrapper_for_install = cfg.bootstrapper.override_install;

    if use_bootstrapper && prefer_bootstrapper_for_install {
        let path = cfg.bootstrapper.path.trim();

        if !path.is_empty() {
            let cli_flag = if cfg.bootstrapper.custom_cli_flag.is_empty() {
                None
            } else {
                Some(cfg.bootstrapper.custom_cli_flag.as_str())
            };

            if bootstrapper::run(path, cli_flag) {
                return InstallLaunch::Bootstrapper;
            }
            return InstallLaunch::None;
        }
    }

    if web::run() {
        InstallLaunch::RobloxInstaller
    } else {
        InstallLaunch::None
    }
}

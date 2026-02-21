use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, SystemTime};

use tracing::{debug, info};

const SESSION_LOG_EARLY_TOLERANCE: Duration = Duration::from_secs(15);
const SESSION_LOG_CANDIDATE_LIMIT: usize = 6;
const SESSION_LOG_CANDIDATE_HARD_LIMIT: usize = 24;
const LOG_TAIL_BYTES: u64 = 128 * 1024;

pub(crate) fn session_has_real_play_markers(
    session_start: Option<SystemTime>,
    active_instances: usize,
) -> bool {
    let Some(start) = session_start else {
        debug!("Skipping Roblox real-play log scan: no session start timestamp");
        return false;
    };

    let cutoff = start
        .checked_sub(SESSION_LOG_EARLY_TOLERANCE)
        .unwrap_or(start);

    let Some(logs_dir) = roblox_logs_dir() else {
        debug!("Skipping Roblox real-play log scan: logs directory not resolved");
        return false;
    };

    let Ok(entries) = fs::read_dir(&logs_dir) else {
        debug!(
            ?logs_dir,
            "Skipping Roblox real-play log scan: failed to read logs directory"
        );
        return false;
    };

    let mut candidates: Vec<(std::path::PathBuf, SystemTime)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("log") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta.modified().ok()?;
            (modified >= cutoff).then_some((path, modified))
        })
        .collect();

    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let candidate_limit = std::cmp::max(SESSION_LOG_CANDIDATE_LIMIT, active_instances.saturating_mul(4))
        .min(SESSION_LOG_CANDIDATE_HARD_LIMIT);

    debug!(
        ?logs_dir,
        active_instances,
        total_candidates = candidates.len(),
        candidate_limit,
        "Scanning Roblox logs for real-play markers"
    );

    let mut marker_hits = 0usize;
    for (path, _) in candidates.into_iter().take(candidate_limit) {
        let has_markers = file_tail_has_real_play_markers(&path, LOG_TAIL_BYTES);
        debug!(?path, has_markers, "Parsed Roblox log candidate for real-play markers");
        if has_markers {
            marker_hits = marker_hits.saturating_add(1);
        }
    }

    if marker_hits > 0 {
        info!(
            marker_hits,
            active_instances,
            "Roblox log scan verified real play markers"
        );
        return true;
    }

    debug!(
        active_instances,
        "Roblox log scan found no real-play markers"
    );
    false
}

fn roblox_logs_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("Roblox").join("logs"))
}

fn file_tail_has_real_play_markers(path: &std::path::Path, tail_bytes: u64) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };

    let Ok(meta) = file.metadata() else {
        return false;
    };

    let len = meta.len();
    let start = len.saturating_sub(tail_bytes);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }

    let tail_len = len.saturating_sub(start);
    let mut buf = Vec::with_capacity(usize::try_from(tail_len).unwrap_or(0));
    if file.read_to_end(&mut buf).is_err() {
        return false;
    }

    let content = String::from_utf8_lossy(&buf);
    let saw_join_initialized = content
        .contains("[FLog::UgcExperienceController] UgcExperienceController: join: initialized dm");
    let saw_surface_replace =
        content.contains("[FLog::SurfaceController] [_:1]::replaceDataModel:");
    let matched = saw_join_initialized || saw_surface_replace;

    debug!(
        ?path,
        saw_join_initialized,
        saw_surface_replace,
        matched,
        "Evaluated real-play markers in Roblox log tail"
    );

    matched
}

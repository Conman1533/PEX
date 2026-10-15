//! glibc keeps freed memory in its arenas, so a server that unloaded most of its chunks keeps
//! its peak RSS. Periodically hand the free pages back to the OS from a background thread.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::time::Duration;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
const TRIM_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(all(target_os = "linux", target_env = "gnu"))]
static SPAWNED: std::sync::Once = std::sync::Once::new();

/// Starts the trimmer once per process. Only does anything on glibc; other allocators return
/// memory themselves.
pub fn spawn_trimmer() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    SPAWNED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("pex-malloc-trim".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(TRIM_INTERVAL);
                    // SAFETY: malloc_trim has no preconditions; it only releases free heap pages.
                    unsafe {
                        libc::malloc_trim(0);
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("Could not start the memory trimmer: {e}");
        }
    });
}

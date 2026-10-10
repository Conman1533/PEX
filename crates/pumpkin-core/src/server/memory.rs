//! glibc keeps freed memory in its arenas, so a server that unloaded most of its chunks keeps
//! its peak RSS. Periodically hand the free pages back to the OS from a background thread.

use std::time::Duration;

const TRIM_INTERVAL: Duration = Duration::from_secs(60);

/// Caps the number of glibc malloc arenas unless `MALLOC_ARENA_MAX` already set one.
/// Call first thing in `main`: a thread only takes an arena on its first allocation.
pub fn configure_allocator() {
    // glibc otherwise creates up to 8 arenas per core and keeps freed pages in each; two kept
    // the peak RSS of a single-player exploration run 13% lower at 2 cores, 17% at 36.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if std::env::var_os("MALLOC_ARENA_MAX").is_none() {
        const MAX_MALLOC_ARENAS: libc::c_int = 2;
        // SAFETY: mallopt only adjusts allocator tuning; it is thread safe and has no preconditions.
        unsafe {
            libc::mallopt(libc::M_ARENA_MAX, MAX_MALLOC_ARENAS);
        }
    }
}

/// Starts the trimmer. Only does anything on glibc; other allocators return memory themselves.
pub fn spawn_trimmer() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
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
    }
}

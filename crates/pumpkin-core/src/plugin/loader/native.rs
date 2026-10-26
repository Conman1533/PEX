use std::{
    any::Any,
    ffi::{CStr, c_char},
    sync::{Arc, LazyLock},
};

use libloading::Library;

use crate::plugin::{
    BUILD_ID, PLUGIN_API_VERSION,
    loader::{PluginLoadFuture, PluginUnloadFuture},
};

use super::{LoaderError, Path, Plugin, PluginLoader, PluginMetadata};

pub struct NativePluginLoader;

impl PluginLoader for NativePluginLoader {
    fn load<'a>(&'a self, path: &'a Path) -> PluginLoadFuture<'a> {
        Box::pin(async {
            let path = path.to_owned();

            // SAFETY: Loading dynamic library from path configured by server administrator.
            let library = unsafe { Library::new(&path) }
                .map_err(|e| LoaderError::LibraryLoad(e.to_string()))?;

            // Checked first: a plugin from another build may lay out every other symbol differently.
            // SAFETY: `PUMPKIN_BUILD_ID` is a NUL-terminated byte array exported by `export_plugin!`
            // and `#[plugin_impl]`, and stays mapped while `library` is alive.
            let plugin_build_id = unsafe {
                library
                    .get::<*const c_char>(b"PUMPKIN_BUILD_ID")
                    .ok()
                    .map(|symbol| CStr::from_ptr(*symbol))
            };
            check_build_id(plugin_build_id.map(CStr::to_bytes), BUILD_ID)?;

            // Ensure this plugin was built against a compatible Pumpkin plugin API version
            // SAFETY: `PUMPKIN_API_VERSION` is an exported `u32` constant symbol created by `#[plugin_impl]`.
            let plugin_api_version = unsafe {
                match library.get::<*const u32>(b"PUMPKIN_API_VERSION") {
                    Ok(symbol) => **symbol,
                    Err(_) => return Err(LoaderError::ApiVersionMissing),
                }
            };

            if plugin_api_version != PLUGIN_API_VERSION {
                return Err(LoaderError::ApiVersionMismatch {
                    plugin_version: plugin_api_version,
                    server_version: PLUGIN_API_VERSION,
                });
            }

            // 2. Extract Metadata (METADATA)
            // `#[plugin_impl]` exports this as a `LazyLock`, since `PluginMetadata`
            // owns its strings and can't be built in a const.
            // SAFETY: `METADATA` is an exported `LazyLock<PluginMetadata>` symbol created by `#[plugin_impl]`.
            let metadata = unsafe {
                let metadata = library
                    .get::<*const LazyLock<PluginMetadata>>(b"METADATA")
                    .map_err(|_| LoaderError::MetadataMissing)?;
                (**metadata).clone()
            };

            // 3. Extract Plugin Factory (plugin)
            // SAFETY: `plugin` is an exported constructor function symbol with signature `fn() -> Box<dyn Plugin>` created by `#[plugin_impl]`.
            let plugin_factory = unsafe {
                library
                    .get::<fn() -> Box<dyn Plugin>>(b"plugin")
                    .map_err(|_| LoaderError::EntrypointMissing)?
            };

            Ok((
                Arc::from(plugin_factory()),
                metadata,
                Box::new(library) as Box<dyn Any + Send + Sync>,
            ))
        })
    }

    fn can_load(&self, path: &Path) -> bool {
        let ext = path.extension().unwrap_or_default();

        if cfg!(target_os = "windows") {
            ext.eq_ignore_ascii_case("dll")
        } else if cfg!(target_os = "macos") {
            ext.eq_ignore_ascii_case("dylib")
        } else {
            ext.eq_ignore_ascii_case("so")
        }
    }

    fn unload(&self, data: Box<dyn Any + Send + Sync>) -> PluginUnloadFuture<'_> {
        Box::pin(async {
            data.downcast::<Library>()
                .map_or(Err(LoaderError::InvalidLoaderData), |library| {
                    drop(library);
                    Ok(())
                })
        })
    }

    /// Windows specific issue: Windows locks DLLs, so we must indicate they cannot be unloaded.
    fn can_unload(&self) -> bool {
        !cfg!(target_os = "windows")
    }
}

/// Refuses a plugin whose build id isn't `server_build_id`. `None` means the plugin exports none.
fn check_build_id(
    plugin_build_id: Option<&[u8]>,
    server_build_id: &str,
) -> Result<(), LoaderError> {
    match plugin_build_id {
        Some(plugin_build_id) if plugin_build_id == server_build_id.as_bytes() => Ok(()),
        Some(plugin_build_id) => Err(LoaderError::BuildIdMismatch {
            plugin_build_id: String::from_utf8_lossy(plugin_build_id).into_owned(),
            server_build_id: server_build_id.to_owned(),
        }),
        None => Err(LoaderError::BuildIdMissing {
            server_build_id: server_build_id.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::{LoaderError, check_build_id};
    use crate::plugin::{BUILD_ID, BUILD_ID_SYMBOL};

    #[test]
    fn mismatched_build_id_is_refused() {
        let server = "commit=2ce43ef3d rustc=1.99.1-58cf8f9c3 target=x86_64-unknown-linux-gnu lock=0123456789abcdef features=none debug-assertions=off";
        let plugin = server.replace("lock=0123456789abcdef", "lock=fedcba9876543210");

        let error = check_build_id(Some(plugin.as_bytes()), server).unwrap_err();

        assert!(matches!(error, LoaderError::BuildIdMismatch { .. }));
        let message = error.to_string();
        assert!(message.contains(&format!("plugin {plugin},")), "{message}");
        assert!(message.contains(&format!("server {server})")), "{message}");
    }

    #[test]
    fn plugin_without_build_id_is_refused() {
        let error = check_build_id(None, BUILD_ID).unwrap_err();

        assert!(matches!(error, LoaderError::BuildIdMissing { .. }));
        assert!(error.to_string().contains(BUILD_ID));
    }

    #[test]
    fn exported_build_id_matches_this_build() {
        let exported = CStr::from_bytes_with_nul(&BUILD_ID_SYMBOL).unwrap();

        assert!(check_build_id(Some(exported.to_bytes()), BUILD_ID).is_ok());
    }
}

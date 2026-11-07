mod library;
mod manifest;
mod provider_settings;
mod registry;
mod remote;
mod state;

pub(crate) use manifest::is_valid_id;
pub use manifest::{Capability, MANIFEST_FILE, Manifest, SCHEMA_VERSION};
pub use provider_settings::{ProviderSettings, SettingsCategory, search_provider_settings};
pub use registry::{
    ProviderRegistry, Refusal, app_registry, bundled_extensions_root, extension_state_root,
    extensions_root,
};
pub use state::{CRASH_LIMIT, State, StateStore, state_path};

use std::fmt;
use std::path::{Path, PathBuf};

use clouddeck_extension_api::API_VERSION;
#[cfg(feature = "test-support")]
use clouddeck_extension_api::abi::Slice;
use libloading::Library;

#[derive(Debug)]
pub enum LoadError {
    Open {
        path: PathBuf,
        source: libloading::Error,
    },
    MissingSymbol {
        name: &'static str,
        source: libloading::Error,
    },
    RelativePath {
        path: PathBuf,
    },
    ApiVersion {
        found: u32,
    },
    BuildInfo {
        found: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Open { path, source } => {
                write!(f, "cannot open extension {}: {source}", path.display())
            }
            LoadError::MissingSymbol { name, source } => {
                write!(f, "extension does not export {name}: {source}")
            }
            LoadError::RelativePath { path } => write!(
                f,
                "extension path {} must be absolute; a bare name is searched for on LD_LIBRARY_PATH",
                path.display()
            ),
            LoadError::ApiVersion { found } => write!(
                f,
                "extension targets boundary version {found}; this host requires {API_VERSION}"
            ),
            LoadError::BuildInfo { found } => write!(
                f,
                "extension was built with `{found}`; the host requires panic=unwind so panics \
                 can be caught at the boundary"
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Open { source, .. } | LoadError::MissingSymbol { source, .. } => {
                Some(source)
            }
            LoadError::RelativePath { .. }
            | LoadError::ApiVersion { .. }
            | LoadError::BuildInfo { .. } => None,
        }
    }
}

pub struct LoadedExtension {
    path: PathBuf,
    // Keeps the shared object mapped; the boundary's function pointers are only valid while it lives.
    _library: Library,
}

impl LoadedExtension {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Tests only: this skips the manifest, containment, ownership and enable checks. The app must go
/// through `ProviderRegistry`.
#[cfg(feature = "test-support")]
pub fn load_extension(path: &Path) -> Result<LoadedExtension, LoadError> {
    if !path.is_absolute() {
        return Err(LoadError::RelativePath {
            path: path.to_path_buf(),
        });
    }
    let library = unsafe { Library::new(path) }.map_err(|source| LoadError::Open {
        path: path.to_path_buf(),
        source,
    })?;

    let api_version = lookup::<unsafe extern "C" fn() -> u32>(&library, "clouddeck_api_version")?;
    check_api_version(unsafe { api_version() })?;

    let build_info = lookup::<unsafe extern "C" fn() -> Slice>(&library, "clouddeck_build_info")?;
    let info = unsafe { build_info().bytes() };
    check_build_info(info)?;

    Ok(LoadedExtension {
        path: path.to_path_buf(),
        _library: library,
    })
}

#[cfg(feature = "test-support")]
fn lookup<F: Copy>(library: &Library, name: &'static str) -> Result<F, LoadError> {
    let symbol = unsafe { library.get::<F>(name.as_bytes()) }
        .map_err(|source| LoadError::MissingSymbol { name, source })?;
    Ok(*symbol)
}

fn check_api_version(found: u32) -> Result<(), LoadError> {
    if found == API_VERSION {
        Ok(())
    } else {
        Err(LoadError::ApiVersion { found })
    }
}

fn check_build_info(info: &[u8]) -> Result<(), LoadError> {
    if info == b"panic=unwind" {
        Ok(())
    } else {
        Err(LoadError::BuildInfo {
            found: String::from_utf8_lossy(info).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_api_version() {
        assert!(check_api_version(API_VERSION).is_ok());
    }

    #[test]
    fn refuses_other_api_versions() {
        assert!(matches!(
            check_api_version(API_VERSION + 1),
            Err(LoadError::ApiVersion { found }) if found == API_VERSION + 1
        ));
    }

    #[test]
    fn refuses_previous_api_version_naming_both_versions() {
        let error = check_api_version(API_VERSION - 1).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "extension targets boundary version {}; this host requires {API_VERSION}",
                API_VERSION - 1
            )
        );
    }

    #[test]
    fn accepts_unwind_builds() {
        assert!(check_build_info(b"panic=unwind").is_ok());
    }

    #[test]
    fn refuses_abort_builds() {
        assert!(matches!(
            check_build_info(b"panic=abort"),
            Err(LoadError::BuildInfo { found }) if found == "panic=abort"
        ));
    }

    #[test]
    fn refuses_unrecognised_build_info() {
        assert!(check_build_info(b"").is_err());
        assert!(check_build_info(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn missing_file_is_an_open_error() {
        let missing = Path::new("/nonexistent/libclouddeck_missing.so");
        assert!(matches!(
            load_extension(missing),
            Err(LoadError::Open { .. })
        ));
    }
}

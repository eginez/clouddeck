use crate::context_path;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Inspector width range in logical pixels.
pub const INSPECTOR_WIDTH_RANGE: (f32, f32) = (300.0, 420.0);
/// Sidebar width range in logical pixels.
pub const SIDEBAR_WIDTH_RANGE: (f32, f32) = (180.0, 320.0);

/// Window position and size in logical pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowFrame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for WindowFrame {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 760.0,
        }
    }
}

/// Browsing context and layout restored when CloudDeck relaunches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    pub selected_regions: Vec<String>,
    pub pinned_searches: Vec<String>,
    pub recent_searches: Vec<String>,
    /// `EC2Instance::id()` keys, most recent first.
    pub recent_instances: Vec<String>,
    pub sidebar_open: bool,
    pub inspector_open: bool,
    pub sidebar_width: f32,
    pub inspector_width: f32,
    pub window: Option<WindowFrame>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_regions: Vec::new(),
            pinned_searches: Vec::new(),
            recent_searches: Vec::new(),
            recent_instances: Vec::new(),
            sidebar_open: true,
            inspector_open: true,
            sidebar_width: 210.0,
            inspector_width: 340.0,
            window: None,
        }
    }
}

impl UiState {
    /// Clamps widths into their ranges and drops a window frame that is non-finite or too small.
    pub fn normalized(self) -> Self {
        let defaults = Self::default();
        let window = self.window.filter(|frame| {
            [frame.x, frame.y, frame.width, frame.height]
                .iter()
                .all(|value| value.is_finite())
                && frame.width >= 520.0
                && frame.height >= 400.0
        });
        Self {
            sidebar_width: clamp_or_default(
                self.sidebar_width,
                SIDEBAR_WIDTH_RANGE,
                defaults.sidebar_width,
            ),
            inspector_width: clamp_or_default(
                self.inspector_width,
                INSPECTOR_WIDTH_RANGE,
                defaults.inspector_width,
            ),
            window,
            ..self
        }
    }

    /// Loads the saved state, falling back to defaults when the file is missing or invalid.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map_or_else(Self::default, Self::normalized)
    }

    /// Writes the state atomically by replacing the file through a temporary sibling.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ui-state.json");
        let temp = path.with_file_name(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::File::create(&temp)?;
        use std::io::Write;
        file.write_all(
            &serde_json::to_vec(&self.clone().normalized()).map_err(std::io::Error::other)?,
        )?;
        file.sync_all()?;
        std::fs::rename(temp, path)
    }
}

/// Path of the UI state file, next to the workspace context file.
pub fn ui_state_path() -> PathBuf {
    context_path().with_file_name("ui-state.json")
}

fn clamp_or_default(value: f32, (min, max): (f32, f32), default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "clouddeck-ui-state-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn default_state_has_empty_lists_open_panes_and_no_window() {
        let state = UiState::default();
        assert!(state.selected_regions.is_empty());
        assert!(state.pinned_searches.is_empty());
        assert!(state.recent_searches.is_empty());
        assert!(state.recent_instances.is_empty());
        assert!(state.sidebar_open);
        assert!(state.inspector_open);
        assert_eq!(state.sidebar_width, 210.0);
        assert_eq!(state.inspector_width, 340.0);
        assert_eq!(state.window, None);
    }

    #[test]
    fn save_then_load_round_trips_state() {
        let path = temp_path("round-trip");
        let state = UiState {
            selected_regions: vec!["us-east-1".into(), "eu-west-1".into()],
            pinned_searches: vec!["state:stopped".into()],
            recent_searches: vec!["owner:esteban".into()],
            recent_instances: vec!["us-east-1:i-1".into()],
            sidebar_open: false,
            inspector_width: 400.0,
            window: Some(WindowFrame {
                x: 10.0,
                y: 20.0,
                width: 1280.0,
                height: 760.0,
            }),
            ..UiState::default()
        };
        state.save(&path).unwrap();
        assert_eq!(UiState::load(&path), state);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_persists_normalized_state_so_non_finite_values_do_not_reset_the_file() {
        let path = temp_path("non-finite");
        let state = UiState {
            recent_searches: vec!["owner:esteban".into()],
            sidebar_width: f32::NAN,
            inspector_width: 999.0,
            window: Some(WindowFrame {
                x: f32::NAN,
                ..WindowFrame::default()
            }),
            ..UiState::default()
        };
        state.save(&path).unwrap();
        let loaded = UiState::load(&path);
        assert_eq!(loaded.recent_searches, vec!["owner:esteban".to_string()]);
        assert_eq!(loaded.sidebar_width, 210.0);
        assert_eq!(loaded.inspector_width, 420.0);
        assert_eq!(loaded.window, None);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let path = temp_path("missing");
        assert_eq!(UiState::load(&path), UiState::default());
    }

    #[test]
    fn load_invalid_json_returns_default() {
        let path = temp_path("invalid");
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(UiState::load(&path), UiState::default());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_partial_json_fills_missing_fields_with_defaults() {
        let path = temp_path("partial");
        std::fs::write(&path, br#"{"sidebar_open":false}"#).unwrap();
        assert_eq!(
            UiState::load(&path),
            UiState {
                sidebar_open: false,
                ..UiState::default()
            }
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalized_clamps_widths() {
        let state = UiState {
            inspector_width: 999.0,
            ..UiState::default()
        }
        .normalized();
        assert_eq!(state.inspector_width, 420.0);

        let state = UiState {
            inspector_width: 10.0,
            ..UiState::default()
        }
        .normalized();
        assert_eq!(state.inspector_width, 300.0);

        let state = UiState {
            inspector_width: f32::NAN,
            ..UiState::default()
        }
        .normalized();
        assert_eq!(state.inspector_width, 340.0);

        let state = UiState {
            sidebar_width: 100.0,
            ..UiState::default()
        }
        .normalized();
        assert_eq!(state.sidebar_width, 180.0);
    }

    #[test]
    fn normalized_drops_window_that_is_too_small_or_non_finite() {
        let frame = |x, y, width, height| {
            UiState {
                window: Some(WindowFrame {
                    x,
                    y,
                    width,
                    height,
                }),
                ..UiState::default()
            }
            .normalized()
            .window
        };
        assert_eq!(frame(0.0, 0.0, 300.0, 760.0), None);
        assert_eq!(frame(0.0, 0.0, 1280.0, 399.0), None);
        assert_eq!(frame(f32::INFINITY, 0.0, 1280.0, 760.0), None);
        assert_eq!(frame(0.0, f32::NAN, 1280.0, 760.0), None);
        assert_eq!(frame(0.0, 0.0, f32::NAN, 760.0), None);
        assert_eq!(frame(0.0, 0.0, 1280.0, f32::NEG_INFINITY), None);
        assert_eq!(frame(0.0, 0.0, 519.9, 760.0), None);
        assert_eq!(
            frame(0.0, 0.0, 520.0, 400.0),
            Some(WindowFrame {
                x: 0.0,
                y: 0.0,
                width: 520.0,
                height: 400.0,
            })
        );
    }

    #[test]
    fn ui_state_path_is_inside_clouddeck_config_directory() {
        assert!(ui_state_path().ends_with("clouddeck/ui-state.json"));
    }
}

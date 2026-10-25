use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub const ICON_PATHS: [&str; 22] = [
    "icons/check.svg",
    "icons/chevron-down.svg",
    "icons/chevron-right.svg",
    "icons/chevron-up.svg",
    "icons/circle-alert.svg",
    "icons/close.svg",
    "icons/cloud.svg",
    "icons/copy.svg",
    "icons/ellipsis.svg",
    "icons/external-link.svg",
    "icons/filter.svg",
    "icons/globe.svg",
    "icons/key.svg",
    "icons/panel-left.svg",
    "icons/panel-right.svg",
    "icons/plug.svg",
    "icons/plus.svg",
    "icons/refresh-cw.svg",
    "icons/search.svg",
    "icons/server.svg",
    "icons/triangle-alert.svg",
    "icons/user.svg",
];

pub const APP_ICON_PATH: &str = "brand/clouddeck-icon-1024.png";

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            APP_ICON_PATH => &include_bytes!("../../../design/icon/clouddeck-icon-1024.png")[..],
            "icons/check.svg" => &include_bytes!("../assets/icons/check.svg")[..],
            "icons/chevron-down.svg" => &include_bytes!("../assets/icons/chevron-down.svg")[..],
            "icons/chevron-right.svg" => &include_bytes!("../assets/icons/chevron-right.svg")[..],
            "icons/chevron-up.svg" => &include_bytes!("../assets/icons/chevron-up.svg")[..],
            "icons/circle-alert.svg" => &include_bytes!("../assets/icons/circle-alert.svg")[..],
            "icons/close.svg" => &include_bytes!("../assets/icons/close.svg")[..],
            "icons/cloud.svg" => &include_bytes!("../assets/icons/cloud.svg")[..],
            "icons/copy.svg" => &include_bytes!("../assets/icons/copy.svg")[..],
            "icons/ellipsis.svg" => &include_bytes!("../assets/icons/ellipsis.svg")[..],
            "icons/external-link.svg" => &include_bytes!("../assets/icons/external-link.svg")[..],
            "icons/filter.svg" => &include_bytes!("../assets/icons/filter.svg")[..],
            "icons/globe.svg" => &include_bytes!("../assets/icons/globe.svg")[..],
            "icons/key.svg" => &include_bytes!("../assets/icons/key.svg")[..],
            "icons/panel-left.svg" => &include_bytes!("../assets/icons/panel-left.svg")[..],
            "icons/panel-right.svg" => &include_bytes!("../assets/icons/panel-right.svg")[..],
            "icons/plug.svg" => &include_bytes!("../assets/icons/plug.svg")[..],
            "icons/plus.svg" => &include_bytes!("../assets/icons/plus.svg")[..],
            "icons/refresh-cw.svg" => &include_bytes!("../assets/icons/refresh-cw.svg")[..],
            "icons/search.svg" => &include_bytes!("../assets/icons/search.svg")[..],
            "icons/server.svg" => &include_bytes!("../assets/icons/server.svg")[..],
            "icons/triangle-alert.svg" => &include_bytes!("../assets/icons/triangle-alert.svg")[..],
            "icons/user.svg" => &include_bytes!("../assets/icons/user.svg")[..],
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, prefix: &str) -> Result<Vec<SharedString>> {
        Ok(ICON_PATHS
            .iter()
            .filter(|path| path.starts_with(prefix))
            .map(|path| SharedString::from(*path))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_path_loads_as_a_24_by_24_currentcolor_svg() {
        for path in ICON_PATHS {
            let bytes = Assets
                .load(path)
                .unwrap()
                .unwrap_or_else(|| panic!("{path} did not load"));
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.starts_with("<svg"), "{path} does not start with <svg");
            assert!(
                text.contains("viewBox=\"0 0 24 24\""),
                "{path} has no 24x24 viewBox"
            );
            assert!(
                text.contains("stroke=\"currentColor\""),
                "{path} is not stroked with currentColor"
            );
            assert!(
                text.trim_end().ends_with("</svg>"),
                "{path} does not end with </svg>"
            );
        }
    }

    #[test]
    fn search_icon_carries_the_mockup_glyph() {
        let bytes = Assets.load("icons/search.svg").unwrap().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains(r#"<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>"#));
    }

    #[test]
    fn app_icon_loads_as_a_png() {
        let bytes = Assets.load(APP_ICON_PATH).unwrap().unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn unknown_paths_load_as_none() {
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }

    #[test]
    fn list_returns_only_icons_under_prefix() {
        assert_eq!(Assets.list("icons/").unwrap().len(), 22);
        assert!(Assets.list("other/").unwrap().is_empty());
    }
}

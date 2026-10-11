use gpui_component::{Icon, IconName};

/// Every icon the UI may use. Source: design/main-window.html; add new ones there first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    PanelLeft,
    PanelRight,
    User,
    Globe,
    Search,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    Copy,
    ExternalLink,
    Close,
    TriangleAlert,
    Check,
    Plus,
    Ellipsis,
    Refresh,
    Cloud,
    Server,
    Filter,
    Plug,
    Key,
    CircleAlert,
}

impl AppIcon {
    pub const ALL: [AppIcon; 22] = [
        AppIcon::PanelLeft,
        AppIcon::PanelRight,
        AppIcon::User,
        AppIcon::Globe,
        AppIcon::Search,
        AppIcon::ChevronDown,
        AppIcon::ChevronRight,
        AppIcon::ChevronUp,
        AppIcon::Copy,
        AppIcon::ExternalLink,
        AppIcon::Close,
        AppIcon::TriangleAlert,
        AppIcon::Check,
        AppIcon::Plus,
        AppIcon::Ellipsis,
        AppIcon::Refresh,
        AppIcon::Cloud,
        AppIcon::Server,
        AppIcon::Filter,
        AppIcon::Plug,
        AppIcon::Key,
        AppIcon::CircleAlert,
    ];

    pub fn path(self) -> &'static str {
        match self {
            Self::PanelLeft => "icons/panel-left.svg",
            Self::PanelRight => "icons/panel-right.svg",
            Self::User => "icons/user.svg",
            Self::Globe => "icons/globe.svg",
            Self::Search => "icons/search.svg",
            Self::ChevronDown => "icons/chevron-down.svg",
            Self::ChevronRight => "icons/chevron-right.svg",
            Self::ChevronUp => "icons/chevron-up.svg",
            Self::Copy => "icons/copy.svg",
            Self::ExternalLink => "icons/external-link.svg",
            Self::Close => "icons/close.svg",
            Self::TriangleAlert => "icons/triangle-alert.svg",
            Self::Check => "icons/check.svg",
            Self::Plus => "icons/plus.svg",
            Self::Ellipsis => "icons/ellipsis.svg",
            Self::Refresh => "icons/refresh-cw.svg",
            Self::Cloud => "icons/cloud.svg",
            Self::Server => "icons/server.svg",
            Self::Filter => "icons/filter.svg",
            Self::Plug => "icons/plug.svg",
            Self::Key => "icons/key.svg",
            Self::CircleAlert => "icons/circle-alert.svg",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::PanelLeft => Icon::new(IconName::PanelLeft),
            Self::PanelRight => Icon::new(IconName::PanelRight),
            Self::User => Icon::new(IconName::User),
            Self::Globe => Icon::new(IconName::Globe),
            Self::Search => Icon::new(IconName::Search),
            Self::ChevronDown => Icon::new(IconName::ChevronDown),
            Self::ChevronRight => Icon::new(IconName::ChevronRight),
            Self::ChevronUp => Icon::new(IconName::ChevronUp),
            Self::Copy => Icon::new(IconName::Copy),
            Self::ExternalLink => Icon::new(IconName::ExternalLink),
            Self::Close => Icon::new(IconName::Close),
            Self::TriangleAlert => Icon::new(IconName::TriangleAlert),
            Self::Check => Icon::new(IconName::Check),
            Self::Plus => Icon::new(IconName::Plus),
            Self::Ellipsis => Icon::new(IconName::Ellipsis),
            _ => Icon::empty().path(self.path()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::ICON_PATHS;
    use gpui_component::IconNamed;

    #[test]
    fn all_lists_22_distinct_icons_with_known_paths() {
        assert_eq!(AppIcon::ALL.len(), 22);
        let mut paths: Vec<&str> = AppIcon::ALL.iter().map(|icon| icon.path()).collect();
        for path in &paths {
            assert!(ICON_PATHS.contains(path), "{path} is not bundled");
        }
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 22);
    }

    #[test]
    fn named_variants_match_gpui_component_icon_names() {
        let pairs = [
            (AppIcon::PanelLeft, IconName::PanelLeft),
            (AppIcon::PanelRight, IconName::PanelRight),
            (AppIcon::User, IconName::User),
            (AppIcon::Globe, IconName::Globe),
            (AppIcon::Search, IconName::Search),
            (AppIcon::ChevronDown, IconName::ChevronDown),
            (AppIcon::ChevronRight, IconName::ChevronRight),
            (AppIcon::ChevronUp, IconName::ChevronUp),
            (AppIcon::Copy, IconName::Copy),
            (AppIcon::ExternalLink, IconName::ExternalLink),
            (AppIcon::Close, IconName::Close),
            (AppIcon::TriangleAlert, IconName::TriangleAlert),
            (AppIcon::Check, IconName::Check),
            (AppIcon::Plus, IconName::Plus),
            (AppIcon::Ellipsis, IconName::Ellipsis),
        ];
        for (app, name) in pairs {
            assert_eq!(app.path().to_string(), name.path().to_string());
        }
    }
}

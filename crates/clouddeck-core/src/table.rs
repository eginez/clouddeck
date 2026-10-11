//! Pure column and cell text rules for the instance table.

use chrono::{DateTime, Utc};

use crate::models::EC2Instance;

/// Instance table column, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Name,
    InstanceId,
    State,
    Type,
    Az,
    PrivateIp,
    PublicIp,
    Launched,
}

impl Column {
    /// Every column in display order.
    pub const ALL: [Column; 8] = [
        Column::Name,
        Column::InstanceId,
        Column::State,
        Column::Type,
        Column::Az,
        Column::PrivateIp,
        Column::PublicIp,
        Column::Launched,
    ];

    /// Header title shown above the column.
    pub fn title(self) -> &'static str {
        match self {
            Column::Name => "Name",
            Column::InstanceId => "Instance ID",
            Column::State => "State",
            Column::Type => "Type",
            Column::Az => "AZ",
            Column::PrivateIp => "Private IP",
            Column::PublicIp => "Public IP",
            Column::Launched => "Launched",
        }
    }

    /// Default column width in points.
    pub fn default_width(self) -> f32 {
        match self {
            Column::Name => 170.0,
            Column::InstanceId => 165.0,
            Column::State => 92.0,
            Column::Type => 100.0,
            Column::Az => 92.0,
            Column::PrivateIp => 104.0,
            Column::PublicIp => 112.0,
            Column::Launched => 130.0,
        }
    }

    /// Whether cells in this column render in a monospace font.
    pub fn monospace(self) -> bool {
        matches!(
            self,
            Column::InstanceId | Column::PrivateIp | Column::PublicIp
        )
    }
}

/// Text for one table cell, with whether it renders muted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellText {
    /// Rendered cell text.
    pub text: String,
    /// Whether the cell renders in the muted style.
    pub muted: bool,
}

/// Text for one instance's cell in the given column.
pub fn cell_text(instance: &EC2Instance, column: Column) -> CellText {
    match column {
        Column::Name => match instance.name().filter(|name| !name.is_empty()) {
            Some(name) => CellText {
                text: name.to_string(),
                muted: false,
            },
            None => CellText {
                text: "unnamed".to_string(),
                muted: true,
            },
        },
        Column::InstanceId => plain_or_dash(Some(instance.instance_id.clone())),
        Column::State => plain_or_dash(instance.state.as_deref().map(capitalize_first)),
        Column::Type => plain_or_dash(instance.instance_type.clone()),
        Column::Az => plain_or_dash(instance.availability_zone.clone()),
        Column::PrivateIp => plain_or_dash(instance.private_ip_address.clone()),
        Column::PublicIp => plain_or_dash(instance.public_ip_address.clone()),
        Column::Launched => plain_or_dash(instance.launch_time.map(|time| {
            DateTime::<Utc>::from(time)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })),
    }
}

/// Columns that fit a window of the given width, dropping from the right as it narrows.
pub fn visible_columns(window_width: f32) -> Vec<Column> {
    Column::ALL
        .into_iter()
        .filter(|column| match column {
            Column::Launched => window_width >= 1100.0,
            Column::PublicIp => window_width >= 960.0,
            Column::PrivateIp => window_width >= 840.0,
            Column::Az => window_width >= 720.0,
            _ => true,
        })
        .collect()
}

fn plain_or_dash(value: Option<String>) -> CellText {
    match value.filter(|value| !value.is_empty()) {
        Some(text) => CellText { text, muted: false },
        None => CellText {
            text: "\u{2014}".to_string(),
            muted: false,
        },
    }
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::fixtures;

    #[test]
    fn titles_and_widths_in_column_order() {
        let titles: Vec<&str> = Column::ALL.iter().map(|c| c.title()).collect();
        assert_eq!(
            titles,
            [
                "Name",
                "Instance ID",
                "State",
                "Type",
                "AZ",
                "Private IP",
                "Public IP",
                "Launched"
            ]
        );
        let widths: Vec<f32> = Column::ALL.iter().map(|c| c.default_width()).collect();
        assert_eq!(
            widths,
            [170.0, 165.0, 92.0, 100.0, 92.0, 104.0, 112.0, 130.0]
        );
    }

    #[test]
    fn monospace_only_for_ids_and_ips() {
        let monospace: Vec<Column> = Column::ALL.into_iter().filter(|c| c.monospace()).collect();
        assert_eq!(
            monospace,
            [Column::InstanceId, Column::PrivateIp, Column::PublicIp]
        );
    }

    #[test]
    fn name_cell_uses_tag_or_muted_unnamed() {
        let fleet = fixtures::fleet();
        assert_eq!(
            cell_text(&fleet[0], Column::Name),
            CellText {
                text: "api-gateway-01".to_string(),
                muted: false
            }
        );
        assert_eq!(
            cell_text(&fleet[3], Column::Name),
            CellText {
                text: "unnamed".to_string(),
                muted: true
            }
        );
    }

    #[test]
    fn state_is_capitalized() {
        let fleet = fixtures::fleet();
        assert_eq!(cell_text(&fleet[0], Column::State).text, "Running");
        assert_eq!(cell_text(&fleet[2], Column::State).text, "Pending");
    }

    #[test]
    fn missing_values_show_dash() {
        let fleet = fixtures::fleet();
        assert_eq!(
            cell_text(&fleet[1], Column::PublicIp),
            CellText {
                text: "\u{2014}".to_string(),
                muted: false
            }
        );
        assert_eq!(cell_text(&fleet[4], Column::Launched).text, "\u{2014}");
        assert_eq!(cell_text(&fleet[4], Column::PrivateIp).text, "\u{2014}");
    }

    #[test]
    fn present_values_are_shown_as_is() {
        let fleet = fixtures::fleet();
        assert_eq!(cell_text(&fleet[0], Column::PublicIp).text, "54.80.2.5");
        assert_eq!(
            cell_text(&fleet[0], Column::Launched).text,
            "2026-10-01 12:00"
        );
        assert_eq!(cell_text(&fleet[1], Column::Az).text, "us-east-1b");
        assert_eq!(
            cell_text(&fleet[2], Column::InstanceId).text,
            "i-0000000000000003"
        );
    }

    #[test]
    fn visible_columns_drop_from_the_right() {
        let count = |width: f32| visible_columns(width).len();
        assert_eq!(count(1280.0), 8);
        assert_eq!(count(1100.0), 8);
        assert_eq!(count(1099.0), 7);
        assert_eq!(count(960.0), 7);
        assert_eq!(count(959.0), 6);
        assert_eq!(count(839.0), 5);
        assert_eq!(
            visible_columns(719.0),
            [
                Column::Name,
                Column::InstanceId,
                Column::State,
                Column::Type
            ]
        );
        assert_eq!(
            visible_columns(520.0),
            [
                Column::Name,
                Column::InstanceId,
                Column::State,
                Column::Type
            ]
        );
    }
}

//! Pure column and cell text rules for the instance table.

use std::cmp::Ordering;
use std::net::Ipv4Addr;

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

/// Columns that fit the table pane, dropping from the right until their widths fit or three remain.
pub fn visible_columns_fit(window_width: f32, pane_width: f32) -> Vec<Column> {
    let mut columns = visible_columns(window_width);
    while columns.len() > 3
        && columns
            .iter()
            .map(|column| column.default_width())
            .sum::<f32>()
            > pane_width
    {
        columns.pop();
    }
    columns
}

/// Column the table is sorted by, and whether the order is ascending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortState {
    /// Column the table is sorted by.
    pub column: Column,
    /// Whether smaller keys come first.
    pub ascending: bool,
}

impl Default for SortState {
    /// Name, ascending.
    fn default() -> Self {
        SortState {
            column: Column::Name,
            ascending: true,
        }
    }
}

impl SortState {
    /// Header click: flips direction on the same column, otherwise sorts ascending by the new column.
    pub fn toggled(self, column: Column) -> SortState {
        if self.column == column {
            SortState {
                column,
                ascending: !self.ascending,
            }
        } else {
            SortState {
                column,
                ascending: true,
            }
        }
    }
}

/// Sorts instances by the sort column; instances missing that value always go last.
pub fn sort_instances(instances: &mut [EC2Instance], sort: SortState) {
    instances.sort_by(|left, right| {
        match (
            has_sort_key(left, sort.column),
            has_sort_key(right, sort.column),
        ) {
            (true, true) => {
                let ordering = compare_present(left, right, sort.column);
                let ordering = if sort.ascending {
                    ordering
                } else {
                    ordering.reverse()
                };
                ordering.then_with(|| left.instance_id.cmp(&right.instance_id))
            }
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => left.instance_id.cmp(&right.instance_id),
        }
    });
}

fn has_sort_key(instance: &EC2Instance, column: Column) -> bool {
    match column {
        Column::Launched => instance.launch_time.is_some(),
        _ => !sort_text(instance, column).is_empty(),
    }
}

fn sort_text(instance: &EC2Instance, column: Column) -> &str {
    let value = match column {
        Column::Name => instance.name(),
        Column::InstanceId => Some(instance.instance_id.as_str()),
        Column::State => instance.state.as_deref(),
        Column::Type => instance.instance_type.as_deref(),
        Column::Az => instance.availability_zone.as_deref(),
        Column::PrivateIp => instance.private_ip_address.as_deref(),
        Column::PublicIp => instance.public_ip_address.as_deref(),
        Column::Launched => None,
    };
    value.unwrap_or_default()
}

/// Compares two instances that both have a sort key for the column.
fn compare_present(left: &EC2Instance, right: &EC2Instance, column: Column) -> Ordering {
    match column {
        Column::Launched => left.launch_time.cmp(&right.launch_time),
        Column::PrivateIp | Column::PublicIp => {
            compare_addresses(sort_text(left, column), sort_text(right, column))
        }
        _ => compare_text(sort_text(left, column), sort_text(right, column)),
    }
}

fn compare_text(left: &str, right: &str) -> Ordering {
    left.chars()
        .map(|c| c.to_ascii_lowercase())
        .cmp(right.chars().map(|c| c.to_ascii_lowercase()))
}

fn compare_addresses(left: &str, right: &str) -> Ordering {
    match (left.parse::<Ipv4Addr>(), right.parse::<Ipv4Addr>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => compare_text(left, right),
    }
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

    #[test]
    fn visible_columns_fit_drops_until_the_pane_fits() {
        let count = |window: f32, pane: f32| visible_columns_fit(window, pane).len();
        assert_eq!(count(1280.0, 1070.0), 8);
        assert_eq!(count(1100.0, 889.0), 7);
        assert_eq!(
            visible_columns_fit(960.0, 750.0),
            [
                Column::Name,
                Column::InstanceId,
                Column::State,
                Column::Type,
                Column::Az,
                Column::PrivateIp
            ]
        );
        assert_eq!(
            visible_columns_fit(1280.0, 730.0),
            visible_columns_fit(960.0, 750.0)
        );
        assert_eq!(count(700.0, 700.0), 4);
        assert_eq!(
            visible_columns_fit(520.0, 520.0),
            [Column::Name, Column::InstanceId, Column::State]
        );
        assert_eq!(
            visible_columns_fit(1100.0, 100.0),
            [Column::Name, Column::InstanceId, Column::State]
        );
    }

    fn sorted_ids(column: Column, ascending: bool) -> Vec<u32> {
        let mut fleet = fixtures::fleet();
        sort_instances(&mut fleet, SortState { column, ascending });
        fleet
            .iter()
            .map(|instance| {
                instance
                    .instance_id
                    .chars()
                    .last()
                    .unwrap()
                    .to_digit(10)
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn default_sort_is_name_ascending() {
        assert_eq!(
            SortState::default(),
            SortState {
                column: Column::Name,
                ascending: true
            }
        );
    }

    #[test]
    fn toggling_same_column_flips_direction() {
        assert_eq!(
            SortState::default().toggled(Column::Name),
            SortState {
                column: Column::Name,
                ascending: false
            }
        );
        let state = SortState {
            column: Column::State,
            ascending: false,
        };
        assert_eq!(
            state.toggled(Column::State),
            SortState {
                column: Column::State,
                ascending: true
            }
        );
    }

    #[test]
    fn toggling_other_column_sorts_it_ascending() {
        let state = SortState {
            column: Column::Name,
            ascending: false,
        };
        assert_eq!(
            state.toggled(Column::State),
            SortState {
                column: Column::State,
                ascending: true
            }
        );
        let state = SortState {
            column: Column::State,
            ascending: true,
        };
        assert_eq!(
            state.toggled(Column::State).toggled(Column::State),
            SortState {
                column: Column::State,
                ascending: true
            }
        );
    }

    #[test]
    fn sorts_by_name() {
        assert_eq!(sorted_ids(Column::Name, true), [1, 2, 5, 3, 4]);
        assert_eq!(sorted_ids(Column::Name, false), [3, 5, 2, 1, 4]);
    }

    #[test]
    fn sorts_by_instance_id() {
        assert_eq!(sorted_ids(Column::InstanceId, true), [1, 2, 3, 4, 5]);
        assert_eq!(sorted_ids(Column::InstanceId, false), [5, 4, 3, 2, 1]);
    }

    #[test]
    fn sorts_by_state_with_id_tiebreak() {
        assert_eq!(sorted_ids(Column::State, true), [3, 1, 4, 2, 5]);
        assert_eq!(sorted_ids(Column::State, false), [2, 5, 1, 4, 3]);
    }

    #[test]
    fn sorts_by_type() {
        assert_eq!(sorted_ids(Column::Type, true), [5, 2, 1, 4, 3]);
        assert_eq!(sorted_ids(Column::Type, false), [3, 4, 1, 2, 5]);
    }

    #[test]
    fn sorts_by_az() {
        assert_eq!(sorted_ids(Column::Az, true), [5, 1, 2, 4, 3]);
    }

    #[test]
    fn sorts_private_ip_numerically() {
        assert_eq!(sorted_ids(Column::PrivateIp, true), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn sorts_public_ip_with_missing_last_in_both_directions() {
        assert_eq!(sorted_ids(Column::PublicIp, true), [1, 3, 2, 4, 5]);
        assert_eq!(sorted_ids(Column::PublicIp, false), [3, 1, 2, 4, 5]);
    }

    #[test]
    fn sorts_by_launch_time() {
        assert_eq!(sorted_ids(Column::Launched, true), [2, 4, 1, 3, 5]);
        assert_eq!(sorted_ids(Column::Launched, false), [3, 1, 4, 2, 5]);
    }
}

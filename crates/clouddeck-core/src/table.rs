//! Pure column and cell text rules for the instance table.

use std::cmp::Ordering;
use std::net::Ipv4Addr;

use chrono::{DateTime, Utc};
use clouddeck_provider_api::ProviderInfo;

use crate::context_model::{short_name, short_name_for};
use crate::instance::native_id;
use crate::{AddressKind, Instance};

pub const PROVIDER_COLUMN_TITLE: &str = "Provider";
pub const PROVIDER_COLUMN_WIDTH: f32 = 64.0;

/// Instance table column, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Provider,
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
    pub const ALL: [Column; 9] = [
        Column::Provider,
        Column::Name,
        Column::InstanceId,
        Column::State,
        Column::Type,
        Column::Az,
        Column::PrivateIp,
        Column::PublicIp,
        Column::Launched,
    ];

    /// Stable key that identifies the column in saved UI state.
    pub fn key(self) -> &'static str {
        match self {
            Column::Provider => "provider",
            Column::Name => "name",
            Column::InstanceId => "instance_id",
            Column::State => "state",
            Column::Type => "type",
            Column::Az => "az",
            Column::PrivateIp => "private_ip",
            Column::PublicIp => "public_ip",
            Column::Launched => "launched",
        }
    }

    /// The column with the given saved key.
    pub fn from_key(key: &str) -> Option<Column> {
        Column::ALL.into_iter().find(|column| column.key() == key)
    }

    /// Header title shown above the column.
    pub fn title(self) -> &'static str {
        match self {
            Column::Provider => PROVIDER_COLUMN_TITLE,
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
            Column::Provider => PROVIDER_COLUMN_WIDTH,
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
pub fn cell_text(instance: &Instance, column: Column) -> CellText {
    match column {
        Column::Provider => CellText {
            text: String::new(),
            muted: false,
        },
        Column::Name => match instance.name.as_deref().filter(|name| !name.is_empty()) {
            Some(name) => CellText {
                text: name.to_string(),
                muted: false,
            },
            None => CellText {
                text: "unnamed".to_string(),
                muted: true,
            },
        },
        Column::InstanceId => plain_or_dash(Some(native_id(instance).to_string())),
        Column::State => plain_or_dash(Some(capitalize_first(&instance.state.raw))),
        Column::Type => plain_or_dash(instance.instance_type.clone()),
        Column::Az => plain_or_dash(instance.location.clone()),
        Column::PrivateIp => {
            plain_or_dash(instance.address(AddressKind::Private).map(str::to_string))
        }
        Column::PublicIp => {
            plain_or_dash(instance.address(AddressKind::Public).map(str::to_string))
        }
        Column::Launched => plain_or_dash(instance.created.map(|time| {
            DateTime::<Utc>::from(time)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })),
    }
}

/// Header for a column; a mixed view across providers uses the shared generic titles.
pub fn column_header(column: Column, mixed: bool) -> &'static str {
    if !mixed {
        return column.title();
    }
    match column {
        Column::Provider => PROVIDER_COLUMN_TITLE,
        Column::Name => "Name",
        Column::InstanceId => "ID",
        Column::State => "State",
        Column::Type => "Type",
        Column::Az => "Location",
        Column::PrivateIp => "Private IP",
        Column::PublicIp => "Public IP",
        Column::Launched => "Launched",
    }
}

/// The Provider column is shown by default only when more than one provider has loaded.
pub fn provider_column_default_visible(loaded_providers: usize) -> bool {
    loaded_providers > 1
}

/// Short provider name shown in the Provider column.
pub fn provider_cell_text(id: &str, name: &str) -> CellText {
    CellText {
        text: short_name_for(id, name),
        muted: false,
    }
}

/// Orders providers by short name, ignoring case.
pub fn compare_provider(a: &ProviderInfo, b: &ProviderInfo) -> Ordering {
    short_name(a)
        .to_lowercase()
        .cmp(&short_name(b).to_lowercase())
}

/// Columns that fit a window of the given width, dropping from the right as it narrows.
pub fn visible_columns(window_width: f32) -> Vec<Column> {
    Column::ALL
        .into_iter()
        .filter(|column| match column {
            Column::Provider => false,
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
    visible_columns_fit_hidden(window_width, pane_width, &[])
}

/// Like [`visible_columns_fit`], after removing the hidden columns. Name is never hidden.
pub fn visible_columns_fit_hidden(
    window_width: f32,
    pane_width: f32,
    hidden: &[Column],
) -> Vec<Column> {
    let mut columns: Vec<Column> = visible_columns(window_width)
        .into_iter()
        .filter(|column| *column == Column::Name || !hidden.contains(column))
        .collect();
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
pub fn sort_instances(instances: &mut [Instance], sort: SortState) {
    instances.sort_by(|left, right| compare_instances(left, right, sort));
}

pub fn compare_instances(left: &Instance, right: &Instance, sort: SortState) -> Ordering {
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
            ordering.then_with(|| native_id(left).cmp(native_id(right)))
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => native_id(left).cmp(native_id(right)),
    }
}

fn has_sort_key(instance: &Instance, column: Column) -> bool {
    match column {
        Column::Launched => instance.created.is_some(),
        _ => !sort_text(instance, column).is_empty(),
    }
}

fn sort_text(instance: &Instance, column: Column) -> &str {
    let value = match column {
        Column::Provider => None,
        Column::Name => instance.name.as_deref(),
        Column::InstanceId => Some(native_id(instance)),
        Column::State => Some(instance.state.raw.as_str()),
        Column::Type => instance.instance_type.as_deref(),
        Column::Az => instance.location.as_deref(),
        Column::PrivateIp => instance.address(AddressKind::Private),
        Column::PublicIp => instance.address(AddressKind::Public),
        Column::Launched => None,
    };
    value.unwrap_or_default()
}

/// Compares two instances that both have a sort key for the column.
fn compare_present(left: &Instance, right: &Instance, column: Column) -> Ordering {
    match column {
        Column::Launched => left.created.cmp(&right.created),
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
                "Provider",
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
            [64.0, 170.0, 165.0, 92.0, 100.0, 92.0, 104.0, 112.0, 130.0]
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
        let fleet = fixtures::fleet_instances();
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
        let fleet = fixtures::fleet_instances();
        assert_eq!(cell_text(&fleet[0], Column::State).text, "Running");
        assert_eq!(cell_text(&fleet[2], Column::State).text, "Pending");
    }

    #[test]
    fn missing_values_show_dash() {
        let fleet = fixtures::fleet_instances();
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
        let fleet = fixtures::fleet_instances();
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

    #[test]
    fn column_keys_round_trip() {
        for column in Column::ALL {
            assert_eq!(Column::from_key(column.key()), Some(column));
        }
        let keys: Vec<&str> = Column::ALL.into_iter().map(Column::key).collect();
        assert_eq!(
            keys,
            [
                "provider",
                "name",
                "instance_id",
                "state",
                "type",
                "az",
                "private_ip",
                "public_ip",
                "launched"
            ]
        );
        assert_eq!(Column::from_key("provider"), Some(Column::Provider));
        assert_eq!(Column::from_key("bogus"), None);
    }

    #[test]
    fn hidden_columns_are_removed_before_width_fitting() {
        for (window, pane) in [
            (1280.0, 1280.0),
            (1280.0, 730.0),
            (960.0, 750.0),
            (520.0, 520.0),
            (1100.0, 100.0),
        ] {
            assert_eq!(
                visible_columns_fit_hidden(window, pane, &[]),
                visible_columns_fit(window, pane)
            );
        }
        assert_eq!(
            visible_columns_fit_hidden(1280.0, 1280.0, &[Column::Az]),
            [
                Column::Name,
                Column::InstanceId,
                Column::State,
                Column::Type,
                Column::PrivateIp,
                Column::PublicIp,
                Column::Launched
            ]
        );
        assert_eq!(
            visible_columns_fit_hidden(1280.0, 730.0, &[Column::InstanceId]),
            [
                Column::Name,
                Column::State,
                Column::Type,
                Column::Az,
                Column::PrivateIp,
                Column::PublicIp
            ]
        );
        assert_eq!(
            visible_columns_fit_hidden(1280.0, 1280.0, &[Column::Name]),
            visible_columns_fit(1280.0, 1280.0)
        );
        let all_but_name: Vec<Column> = Column::ALL
            .into_iter()
            .filter(|column| *column != Column::Name)
            .collect();
        assert_eq!(
            visible_columns_fit_hidden(1280.0, 100.0, &all_but_name),
            [Column::Name]
        );
    }

    fn sorted_ids(column: Column, ascending: bool) -> Vec<u32> {
        let mut fleet = fixtures::fleet_instances();
        sort_instances(&mut fleet, SortState { column, ascending });
        fleet
            .iter()
            .map(|instance| {
                native_id(instance)
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

    fn provider(id: &str, name: &str) -> ProviderInfo {
        ProviderInfo {
            id: id.to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            connection_label: "Profile".to_string(),
        }
    }

    #[test]
    fn single_provider_headers_match_titles() {
        for column in Column::ALL {
            assert_eq!(column_header(column, false), column.title());
        }
    }

    #[test]
    fn mixed_headers_use_generic_names() {
        let headers: Vec<&str> = Column::ALL
            .into_iter()
            .map(|column| column_header(column, true))
            .collect();
        assert_eq!(
            headers,
            [
                "Provider",
                "Name",
                "ID",
                "State",
                "Type",
                "Location",
                "Private IP",
                "Public IP",
                "Launched"
            ]
        );
    }

    #[test]
    fn provider_column_shows_by_default_only_with_several_providers() {
        assert!(!provider_column_default_visible(0));
        assert!(!provider_column_default_visible(1));
        assert!(provider_column_default_visible(2));
    }

    #[test]
    fn provider_cell_uses_short_name() {
        assert_eq!(provider_cell_text("aws", "AWS").text, "AWS");
        assert_eq!(provider_cell_text("gcp", "Google Cloud").text, "GCP");
    }

    #[test]
    fn provider_order_ignores_case() {
        let aws = provider("aws", "AWS");
        let demo = provider("demo", "demo");
        let gcp = provider("gcp", "Google Cloud");
        assert_eq!(compare_provider(&aws, &demo), Ordering::Less);
        assert_eq!(compare_provider(&demo, &gcp), Ordering::Less);
        assert_eq!(compare_provider(&gcp, &demo), Ordering::Greater);
        assert_eq!(
            compare_provider(&provider("x", "aws"), &aws),
            Ordering::Equal
        );
    }
}

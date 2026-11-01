use clouddeck_provider_api::{InspectorRowDef, InspectorSectionDef, InspectorValue};

pub(crate) fn aws_inspector() -> Vec<InspectorSectionDef> {
    vec![
        InspectorSectionDef {
            title: "Instance",
            rows: vec![
                field("State", InspectorValue::State, false, false),
                field("Type", InspectorValue::Type, false, false),
                field("Zone", InspectorValue::Location, false, false),
                extra("AMI", "Instance", "AMI", true, true),
                extra("Key pair", "Instance", "Key pair", false, false),
                field("Launched", InspectorValue::Created, false, false),
            ],
        },
        InspectorSectionDef {
            title: "Network",
            rows: vec![
                field("Private IP", InspectorValue::PrivateAddress, true, true),
                field("Public IP", InspectorValue::PublicAddress, true, true),
                extra("Private DNS", "Network", "Private DNS", true, true),
                extra("VPC", "Network", "VPC", true, true),
                extra("Subnet", "Network", "Subnet", true, true),
            ],
        },
    ]
}

fn field(
    label: &'static str,
    value: InspectorValue,
    monospace: bool,
    copyable: bool,
) -> InspectorRowDef {
    InspectorRowDef {
        label,
        value,
        monospace,
        copyable,
    }
}

fn extra(
    label: &'static str,
    section: &'static str,
    key: &'static str,
    monospace: bool,
    copyable: bool,
) -> InspectorRowDef {
    field(
        label,
        InspectorValue::Extra { section, key },
        monospace,
        copyable,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_inspector_reproduces_todays_layout() {
        let layout: Vec<(&str, Vec<(&str, bool, bool)>)> = aws_inspector()
            .iter()
            .map(|section| {
                (
                    section.title,
                    section
                        .rows
                        .iter()
                        .map(|row| (row.label, row.monospace, row.copyable))
                        .collect(),
                )
            })
            .collect();

        assert_eq!(
            layout,
            vec![
                (
                    "Instance",
                    vec![
                        ("State", false, false),
                        ("Type", false, false),
                        ("Zone", false, false),
                        ("AMI", true, true),
                        ("Key pair", false, false),
                        ("Launched", false, false),
                    ],
                ),
                (
                    "Network",
                    vec![
                        ("Private IP", true, true),
                        ("Public IP", true, true),
                        ("Private DNS", true, true),
                        ("VPC", true, true),
                        ("Subnet", true, true),
                    ],
                ),
            ]
        );
    }

    #[test]
    fn layout_hides_iam_and_public_dns() {
        let hidden = ["IAM instance profile", "Public DNS"];
        let shows_hidden = aws_inspector()
            .iter()
            .flat_map(|section| &section.rows)
            .any(|row| {
                matches!(&row.value, InspectorValue::Extra { key, .. } if hidden.contains(key))
            });
        assert!(!shows_hidden);
    }
}

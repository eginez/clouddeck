use clouddeck_provider_api::{InspectorRowDef, InspectorSectionDef, InspectorValue};

pub fn aws_inspector() -> Vec<InspectorSectionDef> {
    vec![
        InspectorSectionDef {
            title: "Instance".to_string(),
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
            title: "Network".to_string(),
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

fn field(label: &str, value: InspectorValue, monospace: bool, copyable: bool) -> InspectorRowDef {
    InspectorRowDef {
        label: label.to_string(),
        value,
        monospace,
        copyable,
    }
}

fn extra(
    label: &str,
    section: &str,
    key: &str,
    monospace: bool,
    copyable: bool,
) -> InspectorRowDef {
    field(
        label,
        InspectorValue::Extra {
            section: section.to_string(),
            key: key.to_string(),
        },
        monospace,
        copyable,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_inspector_reproduces_todays_layout() {
        type Layout<'a> = Vec<(&'a str, Vec<(&'a str, bool, bool)>)>;
        let sections = aws_inspector();
        let layout: Layout = sections
            .iter()
            .map(|section| {
                (
                    section.title.as_str(),
                    section
                        .rows
                        .iter()
                        .map(|row| (row.label.as_str(), row.monospace, row.copyable))
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
                matches!(&row.value, InspectorValue::Extra { key, .. } if hidden.contains(&key.as_str()))
            });
        assert!(!shows_hidden);
    }
}

use crate::{FieldDef, FieldKind, Instance, MatchMode};

pub fn aws_fields() -> Vec<FieldDef> {
    vec![
        field("vpc", "VPC", "VPC id", true, true, |i| {
            extra(i, "Network", "VPC")
        }),
        field("subnet", "Subnet", "subnet id", true, true, |i| {
            extra(i, "Network", "Subnet")
        }),
        field("sg", "Security groups", "security group", true, true, |i| {
            i.extras
                .iter()
                .find(|section| section.title == "Security groups")
                .map(|section| {
                    section
                        .entries
                        .iter()
                        .flat_map(|(name, id)| [name.clone(), id.clone()])
                        .collect()
                })
                .unwrap_or_default()
        }),
        field("ami", "AMI", "image id", true, true, |i| {
            extra(i, "Instance", "AMI")
        }),
        field("key", "Key pair", "key pair", true, true, |i| {
            extra(i, "Instance", "Key pair")
        }),
        field(
            "iam",
            "IAM instance profile",
            "IAM instance profile",
            false,
            false,
            |i| extra(i, "Instance", "IAM instance profile"),
        ),
        field("dns", "DNS", "private or public DNS", false, false, |i| {
            let mut values = extra(i, "Network", "Private DNS");
            values.extend(extra(i, "Network", "Public DNS"));
            values
        }),
    ]
}

fn field(
    name: &str,
    label: &str,
    hint: &str,
    query: bool,
    search: bool,
    getter: fn(&Instance) -> Vec<String>,
) -> FieldDef {
    FieldDef {
        name: name.to_string(),
        label: label.to_string(),
        hint: hint.to_string(),
        kind: FieldKind::Text,
        match_mode: MatchMode::Contains,
        query,
        search,
        facet: false,
        column: false,
        getter,
    }
}

fn extra(instance: &Instance, section: &str, key: &str) -> Vec<String> {
    instance
        .extras
        .iter()
        .filter(|s| s.title == section)
        .flat_map(|s| s.entries.iter())
        .find(|(entry_key, _)| entry_key == key)
        .map(|(_, value)| vec![value.clone()])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::fixtures::fleet;

    #[test]
    fn aws_fields_cover_aws_extras() {
        let fields = aws_fields();
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["vpc", "subnet", "sg", "ami", "key", "iam", "dns"]);
        let query: Vec<&str> = fields
            .iter()
            .filter(|f| f.query)
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(query, ["vpc", "subnet", "sg", "ami", "key"]);
    }

    #[test]
    fn getters_read_extras() {
        let instance = Instance::from(&fleet()[0]);
        let fields = aws_fields();
        let values = |name: &str| {
            let def = fields.iter().find(|f| f.name == name).unwrap();
            (def.getter)(&instance)
        };
        assert_eq!(values("vpc"), ["vpc-0aaa1111"]);
        assert_eq!(values("subnet"), ["subnet-0bbb2222"]);
        assert_eq!(values("sg"), ["web-sg", "sg-0ccc3333"]);
        assert_eq!(values("ami"), ["ami-0ddd4444"]);
        assert_eq!(values("key"), ["ops-ed25519"]);
        assert_eq!(values("iam"), [""]);
        assert_eq!(values("dns"), ["", ""]);
    }
}

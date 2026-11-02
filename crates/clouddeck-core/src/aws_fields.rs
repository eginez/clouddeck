use crate::{FieldDef, FieldKind, FieldSource, MatchMode};

pub fn aws_fields() -> Vec<FieldDef> {
    vec![
        field("vpc", "VPC", "VPC id", true, true, extra("Network", "VPC")),
        field(
            "subnet",
            "Subnet",
            "subnet id",
            true,
            true,
            extra("Network", "Subnet"),
        ),
        field(
            "sg",
            "Security groups",
            "security group",
            true,
            true,
            FieldSource::Section {
                section: "Security groups",
            },
        ),
        field(
            "ami",
            "AMI",
            "image id",
            true,
            true,
            extra("Instance", "AMI"),
        ),
        field(
            "key",
            "Key pair",
            "key pair",
            true,
            true,
            extra("Instance", "Key pair"),
        ),
        field(
            "iam",
            "IAM instance profile",
            "IAM instance profile",
            false,
            false,
            extra("Instance", "IAM instance profile"),
        ),
        field(
            "dns",
            "DNS",
            "private or public DNS",
            false,
            false,
            FieldSource::Concat(vec![
                extra("Network", "Private DNS"),
                extra("Network", "Public DNS"),
            ]),
        ),
    ]
}

fn field(
    name: &str,
    label: &str,
    hint: &str,
    query: bool,
    search: bool,
    source: FieldSource,
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
        source,
    }
}

fn extra(section: &'static str, key: &'static str) -> FieldSource {
    FieldSource::Extra { section, key }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Instance;
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
    fn sources_read_extras() {
        let instance = Instance::from(&fleet()[0]);
        let fields = aws_fields();
        let values = |name: &str| {
            let def = fields.iter().find(|f| f.name == name).unwrap();
            def.source.values(&instance)
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

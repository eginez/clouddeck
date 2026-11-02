use std::borrow::Borrow;
use std::fmt;

use serde::{Deserialize, Serialize};

pub const INSTANCES: &str = "instances";

macro_rules! string_id {
    ($($name:ident),* $(,)?) => {$(
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    )*};
}

string_id!(
    ConnectionId,
    FlowId,
    KindId,
    ResourceId,
    DimensionId,
    ValueId
);

impl KindId {
    pub fn instances() -> Self {
        Self(INSTANCES.to_string())
    }
}

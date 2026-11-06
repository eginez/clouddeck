use crate::context::AwsProfile;
use crate::demo_scenarios::scenario_data;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoFleet {
    Happy,
    Empty,
    Failure,
    Partial,
    Large,
    Huge,
}

impl DemoFleet {
    pub fn key(self) -> &'static str {
        match self {
            DemoFleet::Happy => "happy",
            DemoFleet::Empty => "empty",
            DemoFleet::Failure => "failure",
            DemoFleet::Partial => "partial",
            DemoFleet::Large => "large",
            DemoFleet::Huge => "huge",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [
            DemoFleet::Happy,
            DemoFleet::Empty,
            DemoFleet::Failure,
            DemoFleet::Partial,
            DemoFleet::Large,
            DemoFleet::Huge,
        ]
        .into_iter()
        .find(|demo| demo.key() == key)
    }
}

pub fn demo_profiles(demo: DemoFleet) -> Vec<AwsProfile> {
    scenario_data(demo)
        .profiles
        .into_iter()
        .map(|profile| AwsProfile {
            name: profile.name,
            region: profile.regions.first().map(|region| region.name.clone()),
        })
        .collect()
}

pub fn demo_region_names(demo: DemoFleet) -> Vec<String> {
    scenario_data(demo)
        .profiles
        .into_iter()
        .flat_map(|profile| profile.regions.into_iter().map(|region| region.name))
        .collect()
}

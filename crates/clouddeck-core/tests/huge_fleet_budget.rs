use std::hint::black_box;
use std::time::{Duration, Instant, SystemTime};

use clouddeck_core::{
    Column, InstanceListViewModel, SortState, compile, fixture_fleet, sort_instances,
};

const QUERIES: [&str; 6] = [
    "h",
    "huge-1",
    "state:running",
    "state:running owner:esteban",
    "state:running owner:esteban web",
    "-state:stopped type:t3",
];
const BUDGET: Duration = Duration::from_millis(50);
const RUNS: usize = 5;

fn keystroke(vm: &mut InstanceListViewModel, text: &str) {
    let now = SystemTime::now();
    vm.set_search_text(text);
    let mut rows = vm.filtered_instances();
    sort_instances(
        &mut rows,
        SortState {
            column: Column::Name,
            ascending: true,
        },
    );
    black_box(&rows);
    black_box(vm.state_facet_counts(now));
    black_box(vm.type_facet_counts(now));
}

#[tokio::test]
async fn keystroke_pipeline_stays_within_budget_on_five_thousand_rows() {
    let mut vm =
        InstanceListViewModel::new(fixture_fleet::static_provider(fixture_fleet::huge_fleet()));
    vm.load_instances().await;
    assert_eq!(vm.instances().len(), 5000);

    for query in QUERIES {
        let best = (0..RUNS)
            .map(|_| {
                let start = Instant::now();
                keystroke(&mut vm, query);
                start.elapsed()
            })
            .min()
            .expect("at least one run");
        assert!(
            best <= BUDGET,
            "query {query:?} took {best:?} at best, over the {BUDGET:?} keystroke budget"
        );
    }
}

#[tokio::test]
async fn indexed_filter_matches_the_linear_filter_on_five_thousand_rows() {
    let fleet = fixture_fleet::huge_fleet();
    let mut vm = InstanceListViewModel::new(fixture_fleet::static_provider(fleet.clone()));
    vm.load_instances().await;
    let now = SystemTime::now();

    for query in QUERIES {
        vm.set_search_text(query);
        let indexed: Vec<String> = vm
            .filtered_instances()
            .iter()
            .map(|i| i.id.clone())
            .collect();
        let linear: Vec<String> = compile(query, vm.known_tag_keys(), now)
            .filter_instances(&fleet)
            .iter()
            .map(|i| i.id.clone())
            .collect();
        assert_eq!(indexed, linear, "query {query:?}");
    }
}

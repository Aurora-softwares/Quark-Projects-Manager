use super::*;
use std::collections::BTreeMap;

fn p(owner: &str, title: &str, number: u64, closed: bool) -> OwnedProject {
    OwnedProject {
        owner: owner.into(),
        project: Project {
            number,
            title: title.into(),
            closed,
            ..Default::default()
        },
    }
}
fn i(title: &str, url: &str, status: &str, priority: &str, size: &str) -> Item {
    Item {
        title: title.into(),
        url: url.into(),
        fields: BTreeMap::from([
            ("Status".into(), status.into()),
            ("Priority".into(), priority.into()),
            ("Size".into(), size.into()),
        ]),
        ..Default::default()
    }
}
fn dataset() -> Vec<OwnedProject> {
    vec![
        p("robert", "My Project", 1, false),
        p("Aurora-Softwares", "Quark IDE", 1, false),
        p("Aurora-Softwares", "Closed", 4, true),
        p("BigOrg", "Hydrogen", 6, false),
    ]
}
#[test]
fn empty_projects_produce_empty_results() {
    assert!(filter_projects(&[], "", "", "robert", Scope::All, true).is_empty());
}
#[test]
fn all_scope_preserves_owner_number_collisions() {
    let projects = dataset();
    let matches = filter_projects(&projects, "", "", "robert", Scope::All, false);
    assert_eq!(matches.iter().filter(|p| p.project.number == 1).count(), 2);
}
#[test]
fn scope_personal_is_case_insensitive() {
    let projects = dataset();
    let matches = filter_projects(&projects, "", "", "ROBERT", Scope::Personal, true);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].owner, "robert");
}
#[test]
fn scope_organisations_excludes_current_username() {
    let projects = dataset();
    let matches = filter_projects(&projects, "", "", "robert", Scope::Organisations, true);
    assert_eq!(matches.len(), 3);
    assert!(!matches.iter().any(|p| p.owner == "robert"));
}
#[test]
fn project_owner_filter_ignores_case() {
    let projects = dataset();
    let matches = filter_projects(
        &projects,
        "",
        "AURORA-SOFTWARES",
        "robert",
        Scope::All,
        true,
    );
    assert_eq!(matches.len(), 2);
}
#[test]
fn project_title_search_is_case_insensitive() {
    let projects = dataset();
    assert_eq!(
        filter_projects(&projects, "QUARK", "", "robert", Scope::All, true).len(),
        1
    );
}
#[test]
fn project_search_by_owner_and_numeric_project_id() {
    let projects = dataset();
    assert_eq!(
        filter_projects(&projects, "bigorg", "", "robert", Scope::All, true).len(),
        1
    );
    assert_eq!(
        filter_projects(&projects, "6", "", "robert", Scope::All, true).len(),
        1
    );
}
#[test]
fn project_search_trim_whitespace() {
    let projects = dataset();
    assert_eq!(
        filter_projects(&projects, "  hydrogen  ", "", "robert", Scope::All, false).len(),
        1
    );
}
#[test]
fn closed_project_is_opt_in() {
    let projects = dataset();
    assert_eq!(
        filter_projects(&projects, "closed", "", "robert", Scope::All, false).len(),
        0
    );
    assert_eq!(
        filter_projects(&projects, "closed", "", "robert", Scope::All, true).len(),
        1
    );
}
#[test]
fn overlapping_scope_owner_and_search_can_eliminate_all() {
    let projects = dataset();
    assert!(filter_projects(
        &projects,
        "quark",
        "robert",
        "robert",
        Scope::Personal,
        true
    )
    .is_empty());
}
#[test]
fn item_filter_empty_set_returns_empty() {
    assert!(filter_items(&[], "", "", "", "").is_empty());
}
#[test]
fn item_filter_empty_filters_return_all() {
    let items = vec![
        i("First", "url", "Ready", "P0", "S"),
        i("Second", "url2", "Done", "P1", "M"),
    ];
    assert_eq!(filter_items(&items, "", "", "", "").len(), 2);
}
#[test]
fn item_filters_are_case_insensitive() {
    let items = vec![i(
        "Hydrogen",
        "https://github.com/o/r/issues/12",
        "In Progress",
        "P0",
        "XL",
    )];
    assert_eq!(
        filter_items(&items, "hydROgen", "in progress", "p0", "xl").len(),
        1
    );
}
#[test]
fn item_search_matches_urls_and_arbitrary_custom_fields() {
    let mut item = i(
        "Hydrogen",
        "https://github.com/o/r/issues/17",
        "Ready",
        "P0",
        "M",
    );
    item.fields.insert("Target Version".into(), "v0.3.1".into());
    let items = vec![item];
    assert_eq!(filter_items(&items, "issues/17", "", "", "").len(), 1);
    assert_eq!(filter_items(&items, "V0.3.1", "", "", "").len(), 1);
}
#[test]
fn item_multiple_filters_are_conjunctive() {
    let items = vec![
        i("one", "", "Ready", "P0", "M"),
        i("two", "", "Ready", "P1", "M"),
        i("three", "", "Done", "P0", "M"),
    ];
    assert_eq!(filter_items(&items, "", "Ready", "P0", "M").len(), 1);
    assert!(filter_items(&items, "three", "Ready", "P0", "M").is_empty());
}
#[test]
fn item_missing_fields_do_not_pass_nonempty_filters() {
    let items = vec![Item {
        title: "missing".into(),
        ..Default::default()
    }];
    assert!(filter_items(&items, "", "Ready", "", "").is_empty());
    assert!(filter_items(&items, "", "", "P0", "").is_empty());
    assert!(filter_items(&items, "", "", "", "M").is_empty());
}
#[test]
fn item_custom_field_key_case_insensitive() {
    let items = vec![Item {
        fields: BTreeMap::from([("pRiOrItY".into(), "P0".into())]),
        ..Default::default()
    }];
    assert_eq!(filter_items(&items, "", "", "p0", "").len(), 1);
}
#[test]
fn results_return_references_to_original_items_not_clones() {
    let projects = dataset();
    let found = filter_projects(&projects, "quark", "", "robert", Scope::All, false);
    assert!(std::ptr::eq(found[0], &projects[1]));
}

use super::*;
use std::collections::BTreeMap;

#[test]
fn large_project_list_filter_is_bounded_by_input() {
    let list: Vec<OwnedProject> = (0..5000)
        .map(|n| OwnedProject {
            owner: if n % 2 == 0 { "Aurora" } else { "Mine" }.into(),
            project: Project {
                number: n,
                title: format!("Project-{n}"),
                closed: n % 7 == 0,
                ..Default::default()
            },
        })
        .collect();
    let selected =
        filters::filter_projects(&list, "Project-42", "Aurora", "Mine", Scope::All, true);
    assert!(selected
        .iter()
        .all(|p| p.owner == "Aurora" && p.project.title.contains("Project-42")));
    assert!(selected.len() <= list.len());
}
#[test]
fn large_issue_list_filters_have_stable_order() {
    let list: Vec<Item> = (0..3000)
        .map(|n| Item {
            title: format!("Issue {n}"),
            fields: BTreeMap::from([(
                "Status".into(),
                if n % 2 == 0 { "Ready" } else { "Done" }.into(),
            )]),
            ..Default::default()
        })
        .collect();
    let selected = filters::filter_items(&list, "Issue 2", "Ready", "", "");
    let indexes: Vec<usize> = selected
        .iter()
        .map(|item| {
            item.title
                .split_whitespace()
                .last()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    assert!(indexes.windows(2).all(|w| w[0] < w[1]));
    assert!(selected.iter().all(|i| i.title.contains("Issue 2")));
}
#[test]
fn five_hundred_csv_rows_are_not_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bulk.csv");
    let mut content = String::from("Title,Priority,Repository\n");
    for n in 0..500 {
        content.push_str(&format!("Task-{n},P1,org/repo\n"));
    }
    std::fs::write(&path, content).unwrap();
    let rows = csv_import::read_csv(&path).unwrap();
    assert_eq!(rows.len(), 500);
    assert_eq!(rows[499].title, "Task-499");
}

//! Pure filtering helpers shared by personal, organisation and combined views.
use crate::github::{Item, Project};

#[derive(Clone, Debug)]
pub struct OwnedProject {
    pub owner: String,
    pub project: Project,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Personal,
    Organisations,
}

pub fn filter_projects<'a>(
    projects: &'a [OwnedProject],
    query: &str,
    owner: &str,
    username: &str,
    scope: Scope,
    show_closed: bool,
) -> Vec<&'a OwnedProject> {
    let needle = query.trim().to_lowercase();
    projects
        .iter()
        .filter(|p| {
            (show_closed || !p.project.closed)
                && (owner.is_empty() || p.owner.eq_ignore_ascii_case(owner))
                && (match scope {
                    Scope::All => true,
                    Scope::Personal => p.owner.eq_ignore_ascii_case(username),
                    Scope::Organisations => !p.owner.eq_ignore_ascii_case(username),
                })
                && (needle.is_empty()
                    || p.project.title.to_lowercase().contains(&needle)
                    || p.owner.to_lowercase().contains(&needle)
                    || p.project.number.to_string().contains(&needle))
        })
        .collect()
}

pub fn filter_items<'a>(
    items: &'a [Item],
    query: &str,
    status: &str,
    priority: &str,
    size: &str,
) -> Vec<&'a Item> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .filter(|item| {
            let value = |key: &str| {
                item.fields
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("")
            };
            (status.is_empty() || value("Status").eq_ignore_ascii_case(status))
                && (priority.is_empty() || value("Priority").eq_ignore_ascii_case(priority))
                && (size.is_empty() || value("Size").eq_ignore_ascii_case(size))
                && (needle.is_empty()
                    || item.title.to_lowercase().contains(&needle)
                    || item.url.to_lowercase().contains(&needle)
                    || item
                        .fields
                        .values()
                        .any(|v| v.to_lowercase().contains(&needle)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn project(owner: &str, name: &str, closed: bool) -> OwnedProject {
        OwnedProject {
            owner: owner.into(),
            project: Project {
                number: 1,
                title: name.into(),
                closed,
                ..Default::default()
            },
        }
    }
    #[test]
    fn scopes_and_owner_filter_correctly() {
        let items = vec![
            project("robert", "Mine", false),
            project("Aurora-softwares", "Quark", false),
            project("OtherOrg", "Other", false),
        ];
        assert_eq!(
            filter_projects(&items, "", "", "robert", Scope::All, false).len(),
            3
        );
        assert_eq!(
            filter_projects(&items, "", "", "robert", Scope::Personal, false).len(),
            1
        );
        assert_eq!(
            filter_projects(&items, "", "", "robert", Scope::Organisations, false).len(),
            2
        );
        assert_eq!(
            filter_projects(&items, "", "Aurora-softwares", "robert", Scope::All, false).len(),
            1
        );
    }
    #[test]
    fn closed_and_search_filters() {
        let items = vec![project("o", "Quark", true), project("o", "Hydrogen", false)];
        assert_eq!(
            filter_projects(&items, "", "", "r", Scope::All, false).len(),
            1
        );
        assert_eq!(
            filter_projects(&items, "quark", "", "r", Scope::All, true).len(),
            1
        );
    }
    #[test]
    fn item_filters_match_multiple_columns() {
        let items = vec![
            Item {
                title: "Compile".into(),
                fields: BTreeMap::from([
                    ("Status".into(), "Ready".into()),
                    ("Priority".into(), "P0".into()),
                    ("Size".into(), "M".into()),
                ]),
                ..Default::default()
            },
            Item {
                title: "Test".into(),
                fields: BTreeMap::from([
                    ("Status".into(), "Done".into()),
                    ("Priority".into(), "P2".into()),
                ]),
                ..Default::default()
            },
        ];
        assert_eq!(filter_items(&items, "", "Ready", "P0", "M").len(), 1);
        assert_eq!(filter_items(&items, "test", "", "", "").len(), 1);
        assert_eq!(filter_items(&items, "", "Ready", "P2", "").len(), 0);
    }
}

#[cfg(test)]
#[path = "tests/filters_extended.rs"]
mod extended_tests;

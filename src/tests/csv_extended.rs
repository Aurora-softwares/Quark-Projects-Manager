use super::*;
use std::fs;
fn parse(data: &str) -> Result<Vec<ImportRow>, String> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("import.csv");
    fs::write(&path, data).unwrap();
    read_csv(&path)
}
#[test]
fn missing_csv_file_produces_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_csv(&dir.path().join("missing.csv"))
        .unwrap_err()
        .contains("Cannot open CSV"));
}
#[test]
fn empty_csv_is_rejected() {
    assert!(parse("").is_err());
}
#[test]
fn titles_are_required_even_when_rows_have_bodies() {
    assert!(parse("Title,Body\n,Description\n \t,Other\n")
        .unwrap_err()
        .contains("no nonempty Title"));
}
#[test]
fn uppercase_and_padded_headers_are_supported() {
    let rows = parse(" TITLE , PRIORITY ,REPOSITORY\n Alpha , P0 , owner/repo\n").unwrap();
    assert_eq!(rows[0].title, "Alpha");
    assert_eq!(rows[0].fields["Priority"], "P0");
}
#[test]
fn byte_order_mark_is_stripped() {
    let rows = parse("\u{feff}Title,Status\nOne,Ready\n").unwrap();
    assert_eq!(rows[0].title, "One");
}
#[test]
fn windows_crlf_is_parsed() {
    let rows = parse("Title,Body\r\nWin,One\r\nLinux,Two\r\n").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].title, "Linux");
}
#[test]
fn embedded_quotes_commas_and_newlines_are_preserved() {
    let rows = parse("Title,Body\n\"He said \"\"hi\"\"\",\"One,two\nthree\"\n").unwrap();
    assert_eq!(rows[0].title, "He said \"hi\"");
    assert_eq!(rows[0].body, "One,two\nthree");
}
#[test]
fn unicode_in_title_and_body_survives() {
    let rows = parse("Title,Body\n世界,Implement naïve café ✓\n").unwrap();
    assert_eq!(rows[0].title, "世界");
    assert!(rows[0].body.contains("✓"));
}
#[test]
fn flexible_extra_columns_are_ignored() {
    let rows = parse("Title,Body\nFoo,Baz,Extra\n").unwrap();
    assert_eq!(rows[0].title, "Foo");
    assert_eq!(rows[0].body, "Baz");
}
#[test]
fn missing_optional_columns_become_empty() {
    let rows = parse("Title\nOnly title\n").unwrap();
    assert!(rows[0].repository.is_empty());
    assert!(rows[0].fields.is_empty());
    assert!(rows[0].body.is_empty());
}
#[test]
fn blank_title_rows_are_skipped() {
    let rows = parse("Title,Priority\n,P1\nReal,P0\n,P2\n").unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn csv_import_does_not_deduplicate_titles_silently() {
    let rows = parse("Title,Priority\nRepeat,P0\nRepeat,P1\n").unwrap();
    assert_eq!(rows.len(), 2);
}
#[test]
fn all_project_fields_have_expected_github_names() {
    let rows =
        parse("Title,Status,Priority,Size,Group,Target Version\nAlpha,Ready,P0,L,Kernel,v0.3\n")
            .unwrap();
    let fields = &rows[0].fields;
    assert_eq!(fields.len(), 5);
    for name in ["Status", "Priority", "Size", "Group", "Target Version"] {
        assert!(fields.contains_key(name));
    }
}
#[test]
fn acceptance_criteria_and_dependencies_are_appended_once() {
    let rows = parse("Title,Body,Acceptance Criteria,Depends On\nAlpha,Body,Works,Beta\n").unwrap();
    assert_eq!(rows[0].body.matches("Acceptance criteria:").count(), 1);
    assert_eq!(rows[0].body.matches("Depends on:").count(), 1);
}
#[test]
fn acceptance_criteria_already_in_body_is_not_repeated() {
    let rows = parse("Title,Body,Acceptance Criteria\nA,Already Works here,Works\n").unwrap();
    assert!(!rows[0].body.contains("Acceptance criteria:"));
}
#[test]
fn type_milestone_repository_are_distinct_from_project_fields() {
    let rows = parse("Title,Type,Milestone,Repository\nA,Bug,v1,aurora/repo\n").unwrap();
    assert_eq!(rows[0].issue_type, "Bug");
    assert_eq!(rows[0].milestone, "v1");
    assert_eq!(rows[0].repository, "aurora/repo");
    assert!(rows[0].fields.is_empty());
}
#[test]
fn invalid_utf8_csv_body_returns_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.csv");
    std::fs::write(&path, b"Title,Body\nValid,\xff\n").unwrap();
    assert!(read_csv(&path).is_err());
}
#[test]
fn repository_validation_accepts_github_identifiers() {
    for value in ["a/b", "Org-Name/Repo.Name", "a_1/b_2", "A123/B123"] {
        assert!(valid_repository(value), "should accept {value}");
    }
}
#[test]
fn repository_validation_rejects_traversal_whitespace_and_shell_chars() {
    for value in [
        "",
        "owner",
        "owner/",
        "/repo",
        "a/b/c",
        "a/..",
        "../repo",
        "a/b c",
        "a/b;rm",
        "a/b\nnext",
        "a/b$var",
        "a/b\\x",
    ] {
        assert!(!valid_repository(value), "should reject {value:?}");
    }
}
#[test]
fn bundled_roadmap_all_rows_have_unique_titles() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rows = read_csv(&root.join("examples/Quark-IDE-Roadmap-GitHub-Import.csv")).unwrap();
    let mut titles = std::collections::HashSet::new();
    for row in &rows {
        assert!(
            titles.insert(row.title.to_lowercase()),
            "Duplicate: {}",
            row.title
        );
    }
}
#[test]
fn bundled_roadmap_has_github_priority_values_and_valid_repositories() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rows = read_csv(&root.join("examples/Quark-IDE-Roadmap-GitHub-Import.csv")).unwrap();
    for row in &rows {
        let p = row.fields.get("Priority").map(String::as_str).unwrap_or("");
        assert!(
            ["P0", "P1", "P2", ""].contains(&p),
            "unexpected priority: {p}"
        );
        assert!(
            valid_repository(&row.repository),
            "{}: {}",
            row.title,
            row.repository
        );
    }
}

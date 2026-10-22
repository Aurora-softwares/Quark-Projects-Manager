use super::*;
use serde_json::json;

#[test]
fn project_parser_rejects_missing_invalid_number() {
    assert!(parse_project(&json!({"title":"Missing"})).is_none());
    assert!(parse_project(&json!({"number":"123"})).is_none());
    assert!(parse_project(&Value::Null).is_none());
}
#[test]
fn project_parser_handles_closed_and_open() {
    assert!(!parse_project(&json!({"number":1})).unwrap().closed);
    assert!(
        parse_project(&json!({"number":1,"closed":true}))
            .unwrap()
            .closed
    );
}
#[test]
fn project_parser_preserves_identity_and_title() {
    let p=parse_project(&json!({"number":77,"id":"PVT_ABC","title":"Hydrogen","url":"https://github.com/orgs/o/projects/77"})).unwrap();
    assert_eq!(p.number, 77);
    assert_eq!(p.id, "PVT_ABC");
    assert_eq!(p.title, "Hydrogen");
    assert!(p.url.ends_with("/77"));
}
#[test]
fn organisations_missing_or_unexpected_shape_returns_empty() {
    for input in ["", "  \n", "{}", r#"[{"login":"not-a-line"}]"#] {
        assert!(parse_organizations(input).is_empty());
    }
}
#[test]
fn organisations_deduplicate_case_insensitively_across_pages() {
    let orgs = parse_organizations("Aurora\nOther\naURORA\nThird\n");
    assert_eq!(orgs.len(), 3);
}
#[test]
fn organisations_sort_case_insensitively() {
    let orgs = parse_organizations("zeta\nAlpha\nbeta\n");
    assert_eq!(orgs, vec!["Alpha", "beta", "zeta"]);
}
#[test]
fn organisations_ignore_empty_memberships() {
    let orgs = parse_organizations("\n  \nreal\n---\ninvalid_name\n");
    assert_eq!(orgs, vec!["real"]);
}
#[test]
fn organisations_handle_windows_line_endings() {
    let orgs = parse_organizations("zeta\r\nAurora\r\n");
    assert_eq!(orgs, vec!["Aurora", "zeta"]);
}
#[test]
fn organisation_scope_errors_give_recovery_instructions() {
    let hint = project_permission_hint(
        "your authentication token is missing required scopes [read:project]",
    );
    assert!(hint.contains("gh auth refresh -s project,read:org"));
    assert!(hint.contains("GH_TOKEN"));
    assert!(project_permission_hint("network connection refused").is_empty());
}
#[test]
fn field_parser_handles_missing_fields_without_panicking() {
    assert!(parse_fields(&Value::Null).is_empty());
    assert!(parse_fields(&json!({})).is_empty());
}
#[test]
fn field_parser_discards_unnamed_fields() {
    let fields = parse_fields(&json!({"fields":[{}, {"name":"Priority","id":"P"}]}));
    assert_eq!(fields.len(), 1);
}
#[test]
fn field_parser_accepts_datatype_and_legacy_type() {
    let fields = parse_fields(
        &json!({"fields":[{"name":"Priority","dataType":"SINGLE_SELECT"},{"name":"Size","type":"ProjectV2SingleSelectField"}]}),
    );
    assert_eq!(fields[0].kind, "SINGLE_SELECT");
    assert_eq!(fields[1].kind, "ProjectV2SingleSelectField");
}
#[test]
fn field_parser_uses_option_ids_and_names() {
    let fields = parse_fields(
        &json!({"fields":[{"id":"F","name":"Status","options":[{"id":"o1","name":"Ready"},{"id":"o2","name":"Done"}]}]}),
    );
    assert_eq!(fields[0].options.len(), 2);
    assert_eq!(fields[0].options[1].id, "o2");
}
#[test]
fn field_value_handles_str_number_bool_and_object() {
    assert_eq!(field_value(&json!("Ready")), "Ready");
    assert_eq!(field_value(&json!(42)), "42");
    assert_eq!(field_value(&json!(true)), "true");
    assert_eq!(field_value(&json!({"name":"P0"})), "P0");
}
#[test]
fn field_value_does_not_leak_arbitrary_json() {
    assert!(field_value(&json!({"private":"TOKEN"})).is_empty());
}
#[test]
fn item_parser_reads_nested_issue_content() {
    let item = parse_item(
        &json!({"id":"I","title":"Bug","type":"Issue","content":{"url":"https://github.com/o/r/issues/7","body":"text","state":"OPEN"}}),
    );
    assert_eq!(item.body, "text");
    assert_eq!(item.state, "OPEN");
    assert_eq!(item.issue_type, "Issue");
}
#[test]
fn item_parser_falls_back_to_top_level_for_drafts() {
    let item = parse_item(
        &json!({"id":"D","title":"Draft","body":"local body","url":"","type":"DraftIssue"}),
    );
    assert_eq!(item.body, "local body");
    assert_eq!(item.issue_type, "DraftIssue");
}
#[test]
fn item_parser_maps_custom_values_and_status() {
    let item = parse_item(
        &json!({"title":"Task","Status":{"name":"Done"},"Priority":"P0","Size":3,"enabled":false,"status":"Ready"}),
    );
    assert_eq!(item.fields["Status"], "Ready");
    assert_eq!(item.fields["Priority"], "P0");
    assert_eq!(item.fields["Size"], "3");
    assert_eq!(item.fields["enabled"], "false");
}
#[test]
fn item_parser_excludes_sensitive_nested_content_as_custom_fields() {
    let item = parse_item(&json!({"content":{"body":"secret"},"title":"Issue"}));
    assert!(!item.fields.contains_key("content"));
    assert!(!item.fields.contains_key("body"));
}
#[test]
fn project_arg_builder_keeps_owner_and_number_separate() {
    let args = project_args("item-list", 42, "Aurora-softwares");
    assert_eq!(
        args,
        vec!["project", "item-list", "42", "--owner", "Aurora-softwares"]
    );
}
#[test]
fn canonical_issue_urls_are_accepted() {
    for url in [
        "https://github.com/Aurora-Softwares/Quark-IDE/issues/1",
        "https://github.com/o/r/issues/99999999",
    ] {
        assert!(valid_issue_url(url), "rejected {url}");
    }
}
#[test]
fn spoofed_issue_urls_are_rejected() {
    for url in [
        "http://github.com/o/r/issues/1",
        "https://github.com.evil.tld/o/r/issues/1",
        "https://github.com@evil.tld/o/r/issues/1",
        "https://github.com/o/r/pulls/1",
        "https://github.com/o/r/issues/0",
        "https://github.com/o/r/issues/1/extra",
        "https://github.com/o/r/issues/1?token=secret",
        "https://github.com/o/r/issues/1#frag",
        "https://github.com/o/r/issues/abc",
        "https://github.com/o/r/issues/1;foo",
        "https://github.com/o/../issues/1",
        "https://github.com/o/r/issues/1\n",
    ] {
        assert!(!valid_issue_url(url), "accepted spoof {url:?}");
    }
}
#[test]
fn issue_parser_normalises_titles_and_deduplicates() {
    let issues = parse_existing_issues(&json!([
        {"title":" Test ","url":"https://github.com/o/r/issues/1"},
        {"title":"TEST","url":"https://github.com/o/r/issues/2"}]));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues["test"], "https://github.com/o/r/issues/1");
}
#[test]
fn issue_parser_skips_invalid_rows_and_url_spoofs() {
    let issues = parse_existing_issues(&json!([
        {}, {"title":"","url":"https://github.com/o/r/issues/1"},
        {"title":"Danger","url":"https://github.com.evil.com/o/r/issues/1"},
        {"title":"Safe","url":"https://github.com/o/r/issues/2"}]));
    assert_eq!(issues.len(), 1);
    assert!(issues.contains_key("safe"));
}
#[test]
fn empty_existing_issue_results_are_safe() {
    assert!(parse_existing_issues(&Value::Null).is_empty());
    assert!(parse_existing_issues(&json!([])).is_empty());
}
#[test]
fn milestone_titles_normalise_and_deduplicate() {
    let names = parse_milestone_titles(&json!([{"title":"v1"},{"title":"V1"},{"title":"v2"}]));
    assert_eq!(names.len(), 2);
    assert!(names.contains("v1"));
}
#[test]
fn milestone_parser_handles_unexpected_response() {
    assert!(parse_milestone_titles(&json!({})).is_empty());
    assert!(parse_milestone_titles(&json!([{}, {"title":""}])).is_empty());
}
#[test]
fn generated_field_edit_args_do_not_split_values() {
    let args = args(&[
        "project",
        "item-edit",
        "--text",
        "a value with spaces;$(rm -rf /)",
    ]);
    assert_eq!(args.len(), 4);
    assert_eq!(args[3], "a value with spaces;$(rm -rf /)");
}

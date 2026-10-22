use super::*;
use serde_json::json;

fn sample_fields() -> Vec<Field> {
    vec![
        Field {
            id: "field_title".into(),
            name: "Title".into(),
            ..Default::default()
        },
        Field {
            id: "field_status".into(),
            name: "Status".into(),
            ..Default::default()
        },
        Field {
            id: "field_priority".into(),
            name: "Priority".into(),
            ..Default::default()
        },
    ]
}

#[test]
fn extended_list_requests_only_custom_fields() {
    let argv = item_list_args("Aurora-softwares", 3, &sample_fields(), true);
    assert_eq!(
        argv,
        vec![
            "project",
            "item-list",
            "3",
            "--owner",
            "Aurora-softwares",
            "--format",
            "json",
            "--limit",
            "10000",
            "--field",
            "Status",
            "--field",
            "Priority",
        ]
    );
}

#[test]
fn basic_list_is_compatible_with_old_cli() {
    let argv = item_list_args("Aurora-softwares", 3, &sample_fields(), false);
    assert_eq!(
        argv,
        vec![
            "project",
            "item-list",
            "3",
            "--owner",
            "Aurora-softwares",
            "--format",
            "json",
            "--limit",
            "10000",
        ]
    );
}

#[test]
fn current_cli_loads_extended_fields_in_one_call() {
    let mut calls = 0;
    let expected = json!({"items":[{"id":"PVTI_1","title":"Test","Priority":"P0"}]});
    let (items, fallback) = list_items_compatible("o", 2, &sample_fields(), |argv| {
        calls += 1;
        assert!(argv.contains(&"--field".to_string()));
        Ok(expected.clone())
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(!fallback);
    assert_eq!(items, expected);
}

#[test]
fn old_cli_recovers_from_unknown_flag_and_keeps_items() {
    let mut calls = 0;
    let (items, fallback) = list_items_compatible("o", 2, &sample_fields(), |argv| {
        calls += 1;
        if argv.contains(&"--field".to_string()) {
            Err("gh project: unknown flag: --field".into())
        } else {
            Ok(json!({"items":[{"id":"PVTI_1","title":"Works","status":"Backlog"}]}))
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert!(fallback);
    assert_eq!(items["items"][0]["title"], "Works");
}

#[test]
fn auth_failures_are_not_silently_retried() {
    let mut calls = 0;
    let result = list_items_compatible("o", 2, &sample_fields(), |_| {
        calls += 1;
        Err("missing required scopes [read:project]".to_owned())
    });
    assert_eq!(calls, 1);
    assert!(result.unwrap_err().contains("read:project"));
}

#[test]
fn when_no_extra_fields_are_available_only_basic_call_runs() {
    let mut calls = 0;
    let fields = vec![Field {
        id: "id1".into(),
        name: "Title".into(),
        ..Default::default()
    }];
    let (_, fallback) = list_items_compatible("o", 2, &fields, |argv| {
        calls += 1;
        assert!(!argv.contains(&"--field".to_string()));
        Ok(json!({"items":[]}))
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert!(!fallback);
}

#[test]
fn fallback_failure_is_returned_to_user() {
    let mut calls = 0;
    let result = list_items_compatible("o", 2, &sample_fields(), |_| {
        calls += 1;
        if calls == 1 {
            Err("unknown flag: --field".into())
        } else {
            Err("network request timed out".into())
        }
    });
    assert_eq!(calls, 2);
    assert!(result.unwrap_err().contains("timed out"));
}

#[test]
fn other_unknown_flags_do_not_trigger_unsafe_fallback() {
    assert!(!unsupported_item_field_flag("unknown flag: --owner"));
    assert!(!unsupported_item_field_flag("failed HTTP 403"));
    assert!(unsupported_item_field_flag(
        "gh project: unknown flag: --field"
    ));
    assert!(unsupported_item_field_flag("UNKNOWN FLAG: '--field'"));
}

#[test]
fn new_cli_custom_values_are_still_parsed() {
    let item = parse_item(&json!({
        "id": "PVTI_1", "title": "Task", "status": "Ready",
        "Priority": "P0", "Size": "M", "Target Version": "0.2.4"
    }));
    assert_eq!(item.fields["Priority"], "P0");
    assert_eq!(item.fields["Size"], "M");
    assert_eq!(item.fields["Target Version"], "0.2.4");
    assert_eq!(item.fields["Status"], "Ready");
}

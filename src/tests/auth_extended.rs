use super::*;
use serde_json::json;

#[test]
fn accounts_missing_hosts_are_empty() {
    assert!(parse_accounts(&json!({})).is_empty());
    assert!(parse_accounts(&json!({"hosts":{"git.example.org":[]}})).is_empty());
    assert!(parse_accounts(&Value::Null).is_empty());
}
#[test]
fn accounts_skip_invalid_logins_and_empty_entries() {
    let accounts = parse_accounts(
        &json!({"hosts":{"github.com":[null,{}, {"login":""}, {"login":5}, {"login":"valid"}]}}),
    );
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].login, "valid");
}
#[test]
fn accounts_default_valid_true_and_inactive() {
    let accounts = parse_accounts(&json!({"hosts":{"github.com":[{"login":"user"}]}}));
    assert!(accounts[0].valid);
    assert!(!accounts[0].active);
}
#[test]
fn accounts_unknown_state_is_invalid() {
    let accounts =
        parse_accounts(&json!({"hosts":{"github.com":[{"login":"user","state":"failure"}]}}));
    assert!(!accounts[0].valid);
}
#[test]
fn account_listing_does_not_copy_credentials() {
    let input = json!({"hosts":{"github.com":[{"login":"alice","oauth_token":"private-secret","token":"secret-123"}]}});
    let accounts = parse_accounts(&input);
    assert!(!format!("{accounts:?}").contains("secret"));
}
#[test]
fn valid_login_accepts_documented_boundaries() {
    assert!(valid_login("a"));
    assert!(valid_login(&"x".repeat(39)));
    assert!(valid_login("user-name-123"));
}
#[test]
fn valid_login_rejects_invalid_boundaries() {
    for name in [
        "",
        "-start",
        "end-",
        "bad space",
        "user/name",
        "evil\nname",
        "é",
        "--help",
    ] {
        assert!(!valid_login(name), "should reject {name:?}");
    }
    assert!(!valid_login(&"x".repeat(40)));
}
#[test]
fn client_id_validation_trims_and_rejects_untrusted_characters() {
    assert_eq!(validate_client_id("  Iv1abc012  ").unwrap(), "Iv1abc012");
    for invalid in ["", "  ", "bad key", "a/b", "a\nb", "abc-1", "id=abc"] {
        assert!(validate_client_id(invalid).is_err(), "accepted {invalid:?}");
    }
}
fn good_code() -> Value {
    json!({"user_code":"ABCD-1234","verification_uri":"https://github.com/login/device",
        "device_code":"opaque-device-secret","expires_in":600,"interval":3})
}
#[test]
fn device_response_reads_expected_fields() {
    let code = parse_device_response(&good_code()).unwrap();
    assert_eq!(code.code, "ABCD-1234");
    assert_eq!(code.uri, "https://github.com/login/device");
    assert_eq!(code.device_code, "opaque-device-secret");
    assert_eq!(code.expires_in, 600);
    assert_eq!(code.interval, 3);
}
#[test]
fn device_response_does_not_leak_device_secret_in_debug() {
    let code = parse_device_response(&good_code()).unwrap();
    let debug = format!("{code:?}");
    assert!(debug.contains("ABCD-1234"));
    assert!(!debug.contains("opaque-device-secret"));
    assert!(debug.contains("[redacted]"));
}
#[test]
fn device_response_rejects_service_error() {
    assert!(
        parse_device_response(&json!({"error":"incorrect_client_credentials"}))
            .unwrap_err()
            .contains("incorrect_client_credentials")
    );
}
#[test]
fn device_response_requires_all_credentials() {
    for name in ["user_code", "verification_uri", "device_code"] {
        let mut value = good_code();
        value.as_object_mut().unwrap().remove(name);
        assert!(parse_device_response(&value).unwrap_err().contains(name));
    }
}
#[test]
fn device_response_applies_safe_polling_defaults() {
    let mut value = good_code();
    value.as_object_mut().unwrap().remove("expires_in");
    value.as_object_mut().unwrap().remove("interval");
    let code = parse_device_response(&value).unwrap();
    assert_eq!(code.expires_in, 900);
    assert_eq!(code.interval, 5);
    value["interval"] = json!(0);
    assert_eq!(parse_device_response(&value).unwrap().interval, 1);
}
#[test]
fn pending_and_rate_limit_device_errors_continue_polling() {
    for error in ["authorization_pending", "slow_down"] {
        assert!(!device_error_action(error).unwrap());
    }
}
#[test]
fn denied_expired_and_unknown_oauth_errors_stop() {
    for error in [
        "access_denied",
        "expired_token",
        "incorrect_client_credentials",
        "unknown",
        "authorization_declined",
    ] {
        assert!(device_error_action(error).is_err());
    }
}

use super::*;
use serde_json::json;

#[test]
fn test_parse() {
    let info = AnnouncementBody::parse(include_str!(
        "../../../samples/service_info_packet_v3_data.json"
    ))
    .unwrap();

    assert_eq!(info.info.identity, "mainapi:4HaM4TN5IVSLNfqhERfKvsVu");
    assert_eq!(info.info.uri, "beepish+tls://172.18.0.7:30201");
    assert_eq!(info.params.weight, 1);
    assert_eq!(info.params.interval, 5000);
    assert!(!info.actions.is_empty());

    for action in &info.actions {
        assert_eq!(action.pathver, format!("{}~{}", action.path, action.version));
    }
}

// T10: RLE decode edge cases
#[test]
fn test_unrle_plain_values() {
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(json!({"v": ["a", "b", "c"]})).unwrap();
    let result: Vec<String> = parse::unrle(&obj, "v", true, 3).unwrap();
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn test_unrle_with_repeats() {
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(json!({"v": [[3, "x"], "y"]})).unwrap();
    let result: Vec<String> = parse::unrle(&obj, "v", true, 4).unwrap();
    assert_eq!(result, vec!["x", "x", "x", "y"]);
}

#[test]
fn test_unrle_missing_optional() {
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(json!({})).unwrap();
    let result: Vec<String> = parse::unrle(&obj, "v", false, 3).unwrap();
    assert_eq!(result, vec!["", "", ""]);
}

#[test]
fn test_unrle_missing_required() {
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(json!({})).unwrap();
    let result: Result<Vec<String>, _> = parse::unrle(&obj, "v", true, 0);
    assert!(result.is_err());
}

#[test]
fn test_unrle_single_value_broadcast() {
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(json!({"v": "same"})).unwrap();
    let result: Vec<String> = parse::unrle(&obj, "v", true, 3).unwrap();
    assert_eq!(result, vec!["same", "same", "same"]);
}

#[test]
fn test_unrle_bad_repeat_chunk() {
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(json!({"v": [[1, 2, 3]]})).unwrap();
    let result: Result<Vec<u32>, _> = parse::unrle(&obj, "v", true, 0);
    assert!(result.is_err());
}

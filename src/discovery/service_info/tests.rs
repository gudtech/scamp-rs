use super::*;

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

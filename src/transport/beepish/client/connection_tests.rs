use crate::service::server_connection;
use crate::test_helpers::echo_actions;
use crate::transport::beepish::proto::{EnvelopeFormat, MessageType};
use std::time::Duration;

use super::connection::ConnectionHandle;

#[tokio::test]
async fn test_client_echo() {
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    let actions = echo_actions();
    let _server = tokio::spawn(server_connection::handle_connection(server_stream, actions, None));

    let conn = ConnectionHandle::from_stream(client_stream);
    let resp = conn
        .send_request(
            "echo",
            1,
            EnvelopeFormat::Json,
            "",
            0,
            b"hello from client".to_vec(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

    assert!(resp.error.is_none());
    assert_eq!(resp.body, b"hello from client");
    assert_eq!(resp.header.message_type, MessageType::Reply);
    assert_eq!(resp.header.request_id.0, 1);
}

#[tokio::test]
async fn test_client_unknown_action_error() {
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    let _server = tokio::spawn(server_connection::handle_connection(server_stream, echo_actions(), None));

    let conn = ConnectionHandle::from_stream(client_stream);
    let resp = conn
        .send_request(
            "nonexistent",
            1,
            EnvelopeFormat::Json,
            "",
            0,
            b"{}".to_vec(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();

    assert!(resp.header.error.as_ref().unwrap().contains("No such action"));
    assert_eq!(resp.header.error_code.as_deref(), Some("not_found"));
}

#[tokio::test]
async fn test_client_large_body() {
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    let _server = tokio::spawn(server_connection::handle_connection(server_stream, echo_actions(), None));

    let body = vec![0xABu8; 5000];
    let conn = ConnectionHandle::from_stream(client_stream);
    let resp = conn
        .send_request("echo", 1, EnvelopeFormat::Json, "", 0, body.clone(), Duration::from_secs(5))
        .await
        .unwrap();

    assert!(resp.error.is_none());
    assert_eq!(resp.body.len(), 5000);
    assert_eq!(resp.body, body);
}

#[tokio::test]
async fn test_client_request_timeout() {
    // Server that accepts but never responds
    let (client_stream, _server_stream) = tokio::io::duplex(65536);
    let conn = ConnectionHandle::from_stream(client_stream);

    let result = conn
        .send_request("echo", 1, EnvelopeFormat::Json, "", 0, b"{}".to_vec(), Duration::from_millis(100))
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("timed out"), "Expected timeout error, got: {}", err);
}

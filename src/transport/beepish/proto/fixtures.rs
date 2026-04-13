//! Wire protocol test fixtures captured from the canonical Perl implementation.
//!
//! These build packets using the exact framing from Perl's Connection.pm:194-195.
//! The JSON payloads use field orderings that Perl's JSON::XS produces.

/// Build a wire packet: `TYPE MSGNO SIZE\r\n<body>END\r\n`
pub fn build_packet(pkt_type: &str, msg_no: u64, body: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(format!("{} {} {}\r\n", pkt_type, msg_no, body.len()).as_bytes());
    buf.extend_from_slice(body);
    buf.extend_from_slice(b"END\r\n");
    buf
}

/// Perl request HEADER with empty strings for ticket/identifying_token.
pub fn perl_request_header() -> Vec<u8> {
    let json = br#"{"envelope":"json","identifying_token":"","request_id":1,"type":"request","client_id":0,"ticket":"","version":1,"action":"api.status.health_check"}"#;
    build_packet("HEADER", 0, json)
}

/// Perl request HEADER with null ticket/identifying_token (from simple_request).
pub fn perl_request_header_with_nulls() -> Vec<u8> {
    let json = br#"{"request_id":1,"client_id":0,"type":"request","version":1,"envelope":"json","ticket":null,"identifying_token":null,"action":"api.status.health_check"}"#;
    build_packet("HEADER", 0, json)
}

/// Perl reply HEADER with null error/error_code (success).
pub fn perl_reply_ok() -> Vec<u8> {
    let json = br#"{"envelope":"json","request_id":1,"error_code":null,"error":null,"type":"reply"}"#;
    build_packet("HEADER", 1, json)
}

/// Perl reply HEADER with error fields.
pub fn perl_reply_error() -> Vec<u8> {
    let json = br#"{"type":"reply","error":"Action not found","error_code":"not_found","envelope":"json","request_id":1}"#;
    build_packet("HEADER", 1, json)
}

/// DATA packet with empty JSON body.
pub fn perl_data_empty_json() -> Vec<u8> {
    build_packet("DATA", 0, b"{}")
}

/// EOF packet (empty body).
pub fn perl_eof() -> Vec<u8> {
    build_packet("EOF", 0, b"")
}

/// ACK packet acknowledging 2 cumulative bytes.
pub fn perl_ack(bytes: u64) -> Vec<u8> {
    build_packet("ACK", 0, bytes.to_string().as_bytes())
}

/// TXERR packet with error message body.
pub fn perl_txerr() -> Vec<u8> {
    build_packet("TXERR", 0, b"Connection closed before message finished")
}

/// A complete request sequence: HEADER + DATA + EOF.
pub fn perl_request_sequence() -> Vec<u8> {
    let mut buf = perl_request_header();
    buf.extend(perl_data_empty_json());
    buf.extend(perl_eof());
    buf
}

/// A complete reply sequence: HEADER + DATA + EOF (for msgno 1).
pub fn perl_reply_sequence(body: &[u8]) -> Vec<u8> {
    let mut buf = perl_reply_ok();
    buf.extend(build_packet("DATA", 1, body));
    buf.extend(build_packet("EOF", 1, b""));
    buf
}

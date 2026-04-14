//! End-to-end integration test for the full SCAMP lifecycle.
//!
//! Proves: cert generation → service bind → announcement → cache file →
//! registry load → TLS request → response. No external dependencies.

use std::io::Write;
use std::net::Ipv4Addr;
use tempfile::NamedTempFile;

use scamp::config::Config;
use scamp::crypto::cert_pem_fingerprint;
use scamp::discovery::ServiceRegistry;
use scamp::service::{ScampReply, ScampService};
use scamp::transport::beepish::proto::EnvelopeFormat;
use scamp::transport::beepish::BeepishClient;

/// Generate a self-signed RSA 2048 certificate + private key (PKCS8 PEM).
fn generate_test_keypair() -> (Vec<u8>, Vec<u8>) {
    use openssl::asn1::Asn1Time;
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::x509::{X509Builder, X509NameBuilder};

    let rsa = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(rsa).unwrap();

    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "scamp-test").unwrap();
    let name = name.build();

    let mut builder = X509Builder::new().unwrap();
    builder.set_version(2).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    builder.set_not_after(&Asn1Time::days_from_now(1).unwrap()).unwrap();
    builder.sign(&pkey, MessageDigest::sha256()).unwrap();

    let cert_pem = builder.build().to_pem().unwrap();
    let key_pem = pkey.private_key_to_pem_pkcs8().unwrap();
    (key_pem, cert_pem)
}

/// Set up a ScampService with an echo handler, bound to localhost.
async fn setup_service() -> (ScampService, Vec<u8>, Vec<u8>) {
    let (key_pem, cert_pem) = generate_test_keypair();
    let mut service = ScampService::new("ScampRsTest", "main");
    service.register("ScampRsTest.echo", 1, |req| async move { ScampReply::ok(req.body) });
    service.bind_pem(&key_pem, &cert_pem, Ipv4Addr::LOCALHOST).await.unwrap();
    (service, key_pem, cert_pem)
}

/// Write synthetic cache and auth files, return Config pointing to them.
/// The temp files are returned to keep them alive for the duration of the test.
fn setup_discovery(announcement_bytes: &[u8], cert_pem: &[u8]) -> (Config, NamedTempFile, NamedTempFile) {
    let cert_pem_str = std::str::from_utf8(cert_pem).unwrap();
    let fingerprint = cert_pem_fingerprint(cert_pem_str).unwrap();

    let mut cache_file = NamedTempFile::new().unwrap();
    cache_file.write_all(announcement_bytes).unwrap();
    write!(cache_file, "\n%%%\n").unwrap();
    cache_file.flush().unwrap();

    let mut auth_file = NamedTempFile::new().unwrap();
    writeln!(auth_file, "{} main:ALL", fingerprint).unwrap();
    auth_file.flush().unwrap();

    let config_str = format!(
        "discovery.cache_path = {}\nbus.authorized_services = {}\n",
        cache_file.path().display(),
        auth_file.path().display(),
    );
    let config = Config::from_content(&config_str).unwrap();
    (config, cache_file, auth_file)
}

/// Test 1: Full echo roundtrip through the entire stack.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_echo_roundtrip() {
    let (service, _key_pem, cert_pem) = setup_service().await;
    let announcement = service.build_announcement_packet(true).unwrap();
    let (config, _cache, _auth) = setup_discovery(&announcement, &cert_pem);

    let registry = ServiceRegistry::new_from_cache(&config).unwrap();
    let entry = registry
        .find_action("main", "ScampRsTest.echo", 1)
        .expect("echo action should be discoverable");

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let service_handle = tokio::spawn(service.run(shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = BeepishClient::new(&config);
    let response = client
        .request(
            &entry.service_info,
            "ScampRsTest.echo",
            1,
            EnvelopeFormat::Json,
            "",
            0,
            b"hello e2e".to_vec(),
            Some(5),
        )
        .await
        .unwrap();

    assert!(
        response.header.error.is_none(),
        "Expected no error, got: {:?}",
        response.header.error
    );
    assert_eq!(response.body, b"hello e2e");

    // Drop client to close connection before shutdown, so drain completes instantly.
    drop(client);
    shutdown_tx.send(true).unwrap();
    service_handle.await.unwrap().unwrap();
}

/// Test 2: Large body (> 2048 bytes) exercises DATA chunking through TLS.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_large_body() {
    let (service, _key_pem, cert_pem) = setup_service().await;
    let announcement = service.build_announcement_packet(true).unwrap();
    let (config, _cache, _auth) = setup_discovery(&announcement, &cert_pem);

    let registry = ServiceRegistry::new_from_cache(&config).unwrap();
    let entry = registry.find_action("main", "ScampRsTest.echo", 1).unwrap();

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let service_handle = tokio::spawn(service.run(shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let body = vec![0x42u8; 5000]; // 3 DATA chunks at 2048 byte chunk size
    let client = BeepishClient::new(&config);
    let response = client
        .request(
            &entry.service_info,
            "ScampRsTest.echo",
            1,
            EnvelopeFormat::Json,
            "",
            0,
            body.clone(),
            Some(5),
        )
        .await
        .unwrap();

    assert!(response.header.error.is_none());
    assert_eq!(response.body.len(), 5000);
    assert_eq!(response.body, body);

    drop(client);
    shutdown_tx.send(true).unwrap();
    service_handle.await.unwrap().unwrap();
}

/// Test 3: Unknown action returns an error in the response header.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unknown_action() {
    let (service, _key_pem, cert_pem) = setup_service().await;
    let announcement = service.build_announcement_packet(true).unwrap();
    let (config, _cache, _auth) = setup_discovery(&announcement, &cert_pem);

    let registry = ServiceRegistry::new_from_cache(&config).unwrap();
    let entry = registry.find_action("main", "ScampRsTest.echo", 1).unwrap();

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let service_handle = tokio::spawn(service.run(shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = BeepishClient::new(&config);
    let response = client
        .request(
            &entry.service_info,
            "NonExistent.action",
            1,
            EnvelopeFormat::Json,
            "",
            0,
            b"{}".to_vec(),
            Some(5),
        )
        .await
        .unwrap();

    // Error is in the response header, not the ScampResponse.error field
    assert!(response.header.error.is_some(), "Expected error for unknown action");
    assert!(response.header.error.as_ref().unwrap().contains("No such action"));

    drop(client);
    shutdown_tx.send(true).unwrap();
    service_handle.await.unwrap().unwrap();
}

/// Test 4: Multiple sequential requests verify connection reuse.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_sequential_requests() {
    let (service, _key_pem, cert_pem) = setup_service().await;
    let announcement = service.build_announcement_packet(true).unwrap();
    let (config, _cache, _auth) = setup_discovery(&announcement, &cert_pem);

    let registry = ServiceRegistry::new_from_cache(&config).unwrap();
    let entry = registry.find_action("main", "ScampRsTest.echo", 1).unwrap();

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let service_handle = tokio::spawn(service.run(shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = BeepishClient::new(&config);
    for i in 0..5 {
        let body = format!("request {}", i).into_bytes();
        let response = client
            .request(
                &entry.service_info,
                "ScampRsTest.echo",
                1,
                EnvelopeFormat::Json,
                "",
                0,
                body.clone(),
                Some(5),
            )
            .await
            .unwrap();
        assert!(response.header.error.is_none(), "Request {} failed: {:?}", i, response.header.error);
        assert_eq!(response.body, body, "Request {} body mismatch", i);
    }

    drop(client);
    shutdown_tx.send(true).unwrap();
    service_handle.await.unwrap().unwrap();
}

/// Test 5: Announcement signature verification on self-signed cert.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_announcement_signature_verification() {
    let (service, _key_pem, cert_pem) = setup_service().await;
    let announcement_bytes = service.build_announcement_packet(true).unwrap();
    let announcement_str = String::from_utf8(announcement_bytes).unwrap();

    let packet = scamp::discovery::packet::AnnouncementPacket::parse(&announcement_str).unwrap();
    assert!(packet.signature_is_valid(), "Self-signed announcement should verify");
    assert_eq!(packet.body.info.identity, service.identity());
    assert_eq!(packet.body.params.weight, 1);

    let cert_pem_str = std::str::from_utf8(&cert_pem).unwrap();
    let expected_fp = cert_pem_fingerprint(cert_pem_str).unwrap();
    assert_eq!(packet.body.info.fingerprint.as_deref(), Some(expected_fp.as_str()));
}

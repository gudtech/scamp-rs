use crate::discovery::ActionEntry;

use anyhow::Result;
use std::{collections::BTreeMap, io::Cursor, sync::Mutex};
use tokio::io::{AsyncRead, AsyncReadExt};

pub struct MockResponse {
    pub headers: BTreeMap<String, String>,
    pub body: Box<dyn AsyncRead + Unpin>,
}

pub struct Expectation {
    pub pathver: String,
    pub req_headers: BTreeMap<String, String>,
    pub req_body: Vec<u8>,
    pub res_headers: BTreeMap<String, String>,
    pub res_body: Vec<u8>,
    pub sleep: Option<u64>,
}

pub struct MockClient {
    expect: Mutex<Vec<Expectation>>,
}

impl MockClient {
    pub fn new() -> Self {
        Self {
            expect: Mutex::new(vec![]),
        }
    }
    pub fn expect(&mut self, expectation: Expectation) {
        self.expect.lock().unwrap().push(expectation);
    }
    pub fn clear(&mut self) {
        self.expect.lock().unwrap().clear();
    }
    pub fn expectations_met(&mut self) -> bool {
        self.expect.lock().unwrap().is_empty()
    }
    pub fn expectation_count(&mut self) -> usize {
        self.expect.lock().unwrap().len()
    }

    pub async fn request(
        &self,
        action: &ActionEntry,
        headers: BTreeMap<String, String>,
        mut body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<MockResponse> {
        eprintln!(
            "  * Mock Call to {} at {}",
            action.action.path, action.service_info.uri
        );

        let pathver = format!("{}~{}", action.action.path, action.action.version);

        let index = {
            let expectations = self.expect.lock().unwrap();
            expectations.iter().position(|e| e.pathver == pathver)
        };

        let expectation = match index {
            Some(idx) => self.expect.lock().unwrap().remove(idx),
            None => {
                return Err(anyhow::anyhow!(
                    "No expectation found for pathver {pathver}"
                ))
            }
        };

        if headers != expectation.req_headers {
            return Err(anyhow::anyhow!("Request headers do not match expectation"));
        }

        println!("    * Request headers match expectation:  {:?}", headers);

        if let Some(sleep) = expectation.sleep {
            tokio::time::sleep(tokio::time::Duration::from_millis(sleep)).await;
        }

        let mut buf = Vec::new();
        body.read_to_end(&mut buf).await.unwrap();

        if buf == expectation.req_body {
            println!("    * Request body matches expectation");
        } else {
            return Err(anyhow::anyhow!("Request body does not match expectation"));
        }

        let mut headers = BTreeMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());

        let body = Box::new(tokio::io::BufReader::new(Cursor::new(expectation.res_body)))
            as Box<dyn AsyncRead + Unpin>;

        println!("    * Response headers: {:?}", headers);
        Ok(MockResponse { headers, body })
    }
}

#[cfg(test)]
mod tests {
    use crate::discovery::*;

    use super::*;

    #[tokio::test]
    async fn test_mock_client() {
        let pathver = "foo.bar~1".to_string();
        let req_headers =
            BTreeMap::from([("content-type".to_string(), "application/json".to_string())]);
        let req_body: Vec<u8> = r#"{"operation":"turboencabulate"}"#.into();
        let res_headers =
            BTreeMap::from([("content-type".to_string(), "application/json".to_string())]);
        let res_body: Vec<u8> = r#"{"status":"great", "reframulation_level": 42}"#.into();

        let mut client = MockClient::new();
        client.expect(Expectation {
            pathver: pathver.clone(),
            req_headers: req_headers.clone(),
            req_body: req_body.clone(),
            res_headers: res_headers.clone(),
            res_body: res_body.clone(),
            sleep: None,
        });

        let service_info = ServiceInfo {
            identity: "test:abcd".to_string(),
            uri: "beepish+tls://127.0.0.1:30100".to_string(),
            fingerprint: None,
        };

        let action = ActionEntry {
            action: service_info::Action {
                path: "foo.bar".to_string(),
                version: 1,
                pathver: "foo.bar~1".to_string(),
                flags: vec![],
                sector: "main".to_string(),
                envelopes: vec!["json".to_string()],
                packet_section: service_info::PacketSection::V3,
            },
            service_info: service_info.clone(),
            announcement_params: service_info::AnnouncementParams {
                weight: 1,
                interval: 5000,
                timestamp: 0.0,
            },
            authorized: true,
        };

        let response = client
            .request(
                &action,
                req_headers.clone(),
                Box::new(std::io::Cursor::new(req_body)),
            )
            .await
            .unwrap();

        let mut body = Vec::new();
        let mut reader = response.body;
        tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut body)
            .await
            .unwrap();
        assert_eq!(body, res_body);
        assert!(client.expectations_met());
    }
}

use crate::discovery::ActionEntry;

use super::{Client, Request, Response};
use anyhow::Result;
use std::{collections::BTreeMap, io::Cursor, sync::Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};

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
}

impl Client for MockClient {
    async fn request<'a>(
        &self,
        action: &'a ActionEntry,
        headers: BTreeMap<String, String>,
        mut body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<Response> {
        eprintln!(
            "  * Mock Call to {} at {}",
            action.action.path, action.service_info.uri
        );

        let pathver = format!("{}~{}", action.action.path, action.action.version);

        // Find the index of the matching expectation
        let index = {
            let expectations = self.expect.lock().unwrap();
            expectations.iter().position(|e| e.pathver == pathver)
        };

        // Remove the expectation if found
        let expectation = match index {
            Some(idx) => self.expect.lock().unwrap().remove(idx),
            None => {
                return Err(anyhow::anyhow!(
                    "No expectation found for pathver {pathver}"
                ))
            }
        };

        // check to see if the request headers match the expectation
        if headers != expectation.req_headers {
            return Err(anyhow::anyhow!("Request headers do not match expectation"));
        }

        println!("    * Request headers match expectation:  {:?}", headers);

        if let Some(sleep) = expectation.sleep {
            tokio::time::sleep(tokio::time::Duration::from_millis(sleep)).await;
        }

        let mut buf = Vec::new();
        body.read_to_end(&mut buf).await.unwrap();

        // check to see if the request body matches the expectation
        if buf == expectation.req_body {
            println!("    * Request body matches expectation");
        } else {
            return Err(anyhow::anyhow!("Request body does not match expectation"));
        }

        // and then return a dummy response
        let mut headers = BTreeMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());

        let body = Box::new(tokio::io::BufReader::new(Cursor::new(expectation.res_body)))
            as Box<dyn AsyncRead + Unpin>;

        // print the response headers
        println!("    * Response headers: {:?}", headers);
        Ok(Response { headers, body })
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
        let action = ActionEntry {
            service_info: ServiceInfo {
                uri: "http://localhost:8080".to_string(),
                identity: "test".to_string(),
            },
            authorized: true,
            announcement_params: AnnouncementParams {
                weight: 1,
                interval: 1,
                timestamp: 1.0,
            },
            action: Action {
                path: "foo.bar".to_string(),
                version: 1,
                pathver: pathver.clone(),
                flags: vec![],
                sector: "".to_string(),
                packet_section: PacketSection::V4,
                envelopes: vec!["json".to_string()],
            },
        };

        client.expect(Expectation {
            pathver,
            req_headers,
            req_body: req_body.clone(),
            res_headers,
            res_body,
            sleep: None,
        });

        client
            .request(
                &action,
                BTreeMap::from([("content-type".to_string(), "application/json".to_string())]),
                Box::new(Cursor::new(req_body)),
            )
            .await
            .unwrap();
    }
}

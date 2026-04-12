use std::collections::BTreeMap;
use std::io::IsTerminal;

use anyhow::Result;
use scamp::config::Config;
use scamp::discovery::service_registry::ServiceRegistry;
use scamp::transport::beepish::proto::EnvelopeFormat;
use scamp::transport::beepish::BeepishClient;
use tokio::io::AsyncBufReadExt;

#[derive(clap::Parser, Debug, Clone)]
pub struct RequestCommand {
    /// The action name we are trying to call, including the version
    /// Example: product.sku.fetch~1
    #[arg(short, long)]
    action: String,

    /// Use a file for the request body
    #[arg(short, long)]
    file: Option<String>,

    /// The request body as a string
    /// either --file or --body must be specified
    /// unless the body is piped to stdin
    #[arg(short, long)]
    pub body: Option<String>,

    /// Add a request header (may specify multiple)
    /// must be in the format of "-H name: value"
    #[arg(short = 'H', long)]
    header: Vec<String>,
}

impl RequestCommand {
    pub async fn run(&self, config: &Config, registry: &ServiceRegistry) -> Result<()> {
        println!("  * Requesting action: {}", self.action);

        let mut parts = self.action.splitn(2, '~');
        let action_name = parts.next().unwrap_or(&self.action);
        let version: i32 = parts.next().unwrap_or("1").parse().unwrap_or(1);

        // Look up action using the new key format (sector:action.vVERSION)
        // Default to "main" sector, matching Perl Requester.pm:15
        let action = registry
            .get_action_by_pathver(&format!("{}~{}", action_name, version), "main")
            .ok_or(anyhow::anyhow!(
                "Action not found: {} (tried sector 'main')",
                action_name
            ))?;

        let mut _headers: BTreeMap<String, String> = BTreeMap::new();
        for header in &self.header {
            let mut parts = header.splitn(2, ':');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                _headers.insert(
                    key.trim().to_string().to_lowercase(),
                    value.trim().to_string(),
                );
            }
        }

        // Get request body
        let is_pipe = !std::io::stdin().is_terminal();
        let body_bytes: Vec<u8> = if let Some(file) = &self.file {
            tokio::fs::read(file).await?
        } else if let Some(body) = &self.body {
            body.clone().into_bytes()
        } else if is_pipe {
            let mut buf = Vec::new();
            let mut stdin = tokio::io::BufReader::new(tokio::io::stdin());
            loop {
                let bytes = stdin.fill_buf().await?;
                if bytes.is_empty() {
                    break;
                }
                buf.extend_from_slice(bytes);
                let len = bytes.len();
                stdin.consume(len);
            }
            buf
        } else {
            return Err(anyhow::anyhow!(
                "Either --file or --body or pipe must be specified"
            ));
        };

        let client = BeepishClient::new(config);

        let response = client
            .request(
                &action.service_info,
                action_name,
                version,
                EnvelopeFormat::Json,
                "", // ticket
                0,  // client_id
                body_bytes,
                None, // use default timeout
            )
            .await?;

        if let Some(err) = &response.error {
            eprintln!("  * Error: {}", err);
        }

        if !response.body.is_empty() {
            print!("{}", String::from_utf8_lossy(&response.body));
        }

        println!(
            "\n  * Request complete. Response contained {} bytes",
            response.body.len()
        );

        Ok(())
    }
}

use std::collections::BTreeMap;

use anyhow::Result;
use scamp::config::Config;
use scamp::discovery::service_registry::ServiceRegistry;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};

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
    // /// use a json file for the headers
    // #[arg(short, long)]
    // header_file: Option<String>,

    // Suppress all output except for the response body
    // #[arg(short, long)]
    // quiet: bool,
}

impl RequestCommand {
    pub async fn run(&self, config: &Config, registry: &ServiceRegistry) -> Result<()> {
        println!("  * Requesting action: {}", self.action);

        // split the given action on the ~ character to get the action name and the version
        // assume version 1 if not specified
        let mut parts = self.action.splitn(2, '~');
        let action_name = parts.next().unwrap_or(&self.action);
        let version = parts.next().unwrap_or("1");

        let pathver = format!("{}~{}", action_name, version);
        // find the action in the registry
        let action = registry
            .get_action(&pathver)
            .ok_or(anyhow::anyhow!("Action not found"))?;

        let mut headers: BTreeMap<String, String> = BTreeMap::new();

        // read in the headers
        for header in &self.header {
            let mut parts = header.splitn(2, ':');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                headers.insert(
                    key.trim().to_string().to_lowercase(),
                    value.trim().to_string(),
                );
            }
        }

        // Get a readable stream for the request body from one of the three sources we support
        let is_pipe = !atty::is(atty::Stream::Stdin);
        let mut body: tokio::io::BufReader<Box<dyn tokio::io::AsyncRead + Send + Unpin>> =
            if let Some(file) = self.file.clone() {
                let file = tokio::fs::File::open(file).await?;
                tokio::io::BufReader::new(Box::new(file))
            } else if let Some(body) = &self.body {
                tokio::io::BufReader::new(Box::new(std::io::Cursor::new(body.clone().into_bytes())))
            } else if is_pipe {
                tokio::io::BufReader::new(Box::new(tokio::io::stdin()))
            } else {
                return Err(anyhow::anyhow!(
                    "Either --file or --body or pipe must be specified"
                ));
            };

        // Peek at the first few bytes to see if we can auto-detect the content type
        let buf = body.fill_buf().await?;

        if !headers.contains_key("content-type") {
            if buf.len() > 0 && buf[0] == b'{' {
                println!(
                    "  * Auto-detected content type as application/json. Override with -H flag"
                );
                headers.insert("content-type".to_string(), "application/json".to_string());
            }
            // Add more content type detection logic based on the peeked bytes if needed
        }

        let client = scamp::transport::beepish::BeepishClient::new(&config);

        use scamp::transport::Client;
        let mut response = client.request(action, headers, Box::new(body)).await?;
        // print the response body
        let mut bytes = 0;
        let mut buffer = [0; 1024];
        loop {
            match response.body.read(&mut buffer).await {
                Ok(0) => break,
                Ok(bytes_read) => {
                    bytes += bytes_read;
                    print!("{}", String::from_utf8_lossy(&buffer[..bytes_read]));
                }
                Err(e) => eprintln!("Error reading body: {}", e),
            }
        }

        println!("\n  *  Request complete. Response contained {bytes} bytes");

        Ok(())
    }
}

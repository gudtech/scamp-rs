use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom};

use anyhow::Result;
use regex::Regex;
use scamp::config::Config;
use scamp::discovery::service_registry::ServiceRegistry;
use scamp::transport::RequestHeader;

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
    #[arg(short, long)]
    pub body: String,

    /// Add a request header (may specify multiple)
    /// must be in the format of "-H name: value"
    #[arg(short = 'H', long)]
    header: Vec<String>,
    // /// use a json file for the headers
    // #[arg(short, long)]
    // header_file: Option<String>,
}

// Add this new trait definition
trait BufReadSeek: BufRead + Seek {}
impl<T: BufRead + Seek> BufReadSeek for T {}

impl RequestCommand {
    pub fn run(&self, _config: &Config, registry: &ServiceRegistry) -> Result<()> {
        println!("Requesting action: {}", self.action);

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

        // get a readable stream for the body
        let mut reader: Box<dyn BufReadSeek> = if let Some(file) = self.file.clone() {
            let file = File::open(file)?;
            Box::new(BufReader::new(file))
        } else {
            Box::new(BufReader::new(Cursor::new(self.body.as_bytes())))
        };

        // get a buffered reader and peek the first character to see if it's a `{`
        // and auto-detect the content type as application/json unless there's already a content type header
        // Peek at the first byte
        let mut first_byte = [0];
        reader.read_exact(&mut first_byte)?;
        // Reset the reader to the beginning
        reader.seek(SeekFrom::Start(0))?;

        if !headers.contains_key("content-type") {
            if first_byte[0] == b'{' {
                println!("Auto-detected content type as application/json. Override with -H flag");
                headers.insert("content-type".to_string(), "application/json".to_string());
            }
        }

        println!("Headers: {:?}", headers);

        // stream the body to screen as a way to test the body
        let mut buffer = [0; 1024];
        loop {
            let bytes = reader.read(&mut buffer)?;
            if bytes == 0 {
                break;
            }
            print!("{}", String::from_utf8_lossy(&buffer[..bytes]));
        }
        Ok(())
    }
}

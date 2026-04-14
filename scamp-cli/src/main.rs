use anyhow::Result;
use clap::{Parser, Subcommand};
use list::ListCommand;
use request::RequestCommand;
use scamp::{config::Config, discovery::service_registry::ServiceRegistry};
use serve::ServeCommand;
mod list;
mod request;
mod serve;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
    /// Use a specific config file
    #[arg(short, long)]
    config: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List actions or services
    List {
        #[command(subcommand)]
        command: ListCommand,
    },
    /// Make a request to a service
    Request(RequestCommand),
    /// Start a test service
    Serve(ServeCommand),
}

impl Commands {
    async fn run(&self, config: &Config) -> Result<()> {
        match self {
            Commands::List { command } => {
                let registry = ServiceRegistry::new_from_cache(config)?;
                command.run(config, &registry)
            }
            Commands::Request(command) => {
                let registry = ServiceRegistry::new_from_cache(config)?;
                command.run(config, &registry).await
            }
            Commands::Serve(command) => command.run(config).await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let config = Config::new(args.config)?;
    args.command.run(&config).await
}

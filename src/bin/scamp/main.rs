use anyhow::Result;
use clap::{Parser, Subcommand};
use list::ListCommand;
use request::RequestCommand;
use scamp::{config::Config, discovery::service_registry::ServiceRegistry};
mod list;
mod request;

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
}

impl Commands {
    fn run(&self, config: &Config, registry: &ServiceRegistry) -> Result<()> {
        match self {
            Commands::List { command } => command.run(config, registry),
            Commands::Request(command) => command.run(config, registry),
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config = Config::new(args.config)?;
    let registry = ServiceRegistry::new_from_cache(&config)?;
    args.command.run(&config, &registry)
}

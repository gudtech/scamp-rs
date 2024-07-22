use anyhow::Result;
use clap::{Parser, Subcommand};
use scamp::config::Config;

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
}

#[derive(Subcommand, Debug)]
enum ListCommand {
    /// List actions of some service
    #[command(aliases = ["a", "ac","act", "action"])]
    Actions {
        /// The service to list actions for
        #[arg(short, long)]
        service: Option<String>,

        /// Show unauthorized actions
        #[arg(short, long)]
        all: bool,

        /// Tab-delimited output for parsing
        #[arg(long)]
        raw: bool,

        /// Do not truncate columns
        #[arg(short, long)]
        verbose: bool,
    },
    /// List services
    #[command(aliases = ["s","serv","svc", "service"])]
    Services {
        /// List the services that offer an action (prefix match)
        #[arg(short = 'o', long)]
        offers: Option<String>,

        /// Show unauthorized services
        #[arg(short, long)]
        all: bool,

        /// Restrict to services from a given host (prefix match)
        #[arg(long)]
        host: Option<String>,

        /// Select services by name (prefix match)
        #[arg(long)]
        name: Option<String>,

        /// Tab-delimited output for parsing
        #[arg(long)]
        raw: bool,

        /// Do not truncate columns
        #[arg(short, long)]
        verbose: bool,

        /// Restrict to services advertising a specific action
        #[arg(long)]
        with_action: Option<String>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("{:?}", args);

    let config = Config::new(args.config)?;

    // this is an error if we don't have a cache path
    let cache_path = config
        .get("discovery.cache_path")
        .ok_or(anyhow::anyhow!("No cache path found"))?;

    let infos = 

    match args.command {
        Commands::List { command } => match command {
            ListCommand::Actions { .. } => {}
            ListCommand::Services { .. } => todo!(),
        },
    }
    Ok(())
}

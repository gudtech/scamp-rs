use anyhow::Result;
use clap::{Parser, Subcommand};
use scamp::{config::Config, discovery::service_registry::ServiceRegistry};
use term_table::{row::Row, table_cell::TableCell, Table};

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
        /// search for actions which contain this string (partial match)
        #[arg(short, long)]
        name: Option<String>,

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

    let config = Config::new(args.config)?;
    let registry = ServiceRegistry::new_from_cache(&config)?;

    match args.command {
        Commands::List { command } => match command {
            ListCommand::Actions {
                name,
                service,
                all,
                raw,
                verbose,
            } => {
                // normalize name into lowercase with slashes replaced with .
                let name = name.map(|n| n.to_lowercase().replace('/', "."));

                // print the table header
                let mut table = Table::new();
                let mut headers = vec![TableCell::new("Name"), TableCell::new("Service")];
                if all {
                    headers.push(TableCell::new("Authorized"));
                }
                if verbose {
                    // Add other fields when verbose is set
                    // headers.push(TableCell::new("OtherField1"));
                    // headers.push(TableCell::new("OtherField2"));
                }
                if !raw {
                    table.add_row(Row::new(headers));
                }

                let mut i = 0;

                // implement each filter from the clap args
                for action in registry.actions_iter() {
                    if let Some(name) = &name {
                        if !action.action.pathver.contains(name) {
                            continue;
                        }
                    }
                    if let Some(service) = &service {
                        if !action.service_info.identity.starts_with(service) {
                            continue;
                        }
                    }
                    if !all && !action.authorized {
                        continue;
                    }
                    i += 1;
                    let mut row = vec![
                        TableCell::new(action.action.pathver.clone()),
                        TableCell::new(action.service_info.identity.clone()),
                    ];
                    if all {
                        row.push(TableCell::new(action.authorized.to_string()));
                    }
                    if raw {
                        println!("{:?}", row);
                    } else {
                        let mut row = Row::new(row);
                        if i > 1 {
                            row.has_separator = false;
                        }
                        table.add_row(row);
                    }
                }
                if !raw {
                    println!("{}", table.render());
                }
            }
            ListCommand::Services { .. } => todo!(),
        },
    }
    Ok(())
}

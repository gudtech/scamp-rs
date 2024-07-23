use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use clap::Subcommand;
use scamp::config::Config;
use scamp::discovery::service_registry::ServiceRegistry;
use term_table::{row::Row, table_cell::TableCell, Table};

#[derive(Subcommand, Debug)]
pub enum ListCommand {
    /// List actions of some service
    #[command(aliases = ["a", "ac","act", "action"])]
    Actions {
        /// Include only actions whose path contains this string (partial match)
        #[arg(short, long)]
        name: Option<String>,

        /// Include only actions for this service (prefix match)
        #[arg(short = 'S', long)]
        service: Option<String>,

        /// Include only actions whose sector contains this string (partial match)
        #[arg(short = 's', long)]
        sector: Option<String>,

        /// Also include unauthorized actions
        #[arg(short, long)]
        all: bool,

        /// Tab-delimited output for parsing
        #[arg(long)]
        raw: bool,
    },
    /// List services
    #[command(aliases = ["s","serv","svc", "service"])]
    Services {
        /// Restrict to services advertising a specific action
        #[arg(long)]
        with_action: Option<String>,

        /// Include only actions whose sector contains this string (partial match)
        #[arg(short = 's', long)]
        sector: Option<String>,

        /// Show unauthorized services
        #[arg(short, long)]
        all: bool,

        /// Restrict to services from a given uri (partial match)
        #[arg(long)]
        uri: Option<String>,

        /// Select services by name (partial match)
        #[arg(long)]
        name: Option<String>,

        /// Tab-delimited output for parsing
        #[arg(long)]
        raw: bool,
    },
    /// List sectors
    #[command(aliases = ["sec", "sector"])]
    Sectors {
        /// Tab-delimited output for parsing
        #[arg(long)]
        raw: bool,
    },
}

impl ListCommand {
    pub fn run(&self, config: &Config, registry: &ServiceRegistry) -> Result<()> {
        match self {
            ListCommand::Actions { .. } => self.list_actions(config, registry),
            ListCommand::Services { .. } => self.list_services(config, registry),
            ListCommand::Sectors { .. } => self.list_sectors(config, registry),
        }
    }
    fn list_actions(&self, _config: &Config, registry: &ServiceRegistry) -> Result<()> {
        let ListCommand::Actions {
            sector,
            name,
            service,
            all,
            raw,
        } = self
        else {
            return Err(anyhow::anyhow!("Invalid command"));
        };

        let mut services = HashSet::new();
        // normalize name into lowercase with slashes replaced with .
        let name = name.as_ref().map(|n| n.to_lowercase().replace('/', "."));
        let mut table = Table::new();
        let mut headers = vec![TableCell::new("Name"), TableCell::new("Service")];
        if *all {
            headers.push(TableCell::new("Authorized"));
        }
        if !raw {
            table.add_row(Row::new(headers));
        }

        let mut i = 0;

        // implement each filter from the clap args
        for ae in registry.actions_iter() {
            if let Some(sector) = &sector {
                if !ae.action.sector.contains(sector) {
                    continue;
                }
            }
            if let Some(name) = &name {
                if !ae.action.pathver.contains(name) {
                    continue;
                }
            }
            if let Some(service) = &service {
                if !ae.service_info.identity.starts_with(service) {
                    continue;
                }
            }
            if !all && !ae.authorized {
                continue;
            }
            i += 1;
            services.insert(ae.service_info.identity.clone());

            if *raw {
                // just include all fields in a single println - tab delimited
                println!(
                    "{}\t{}\t{}\t{}",
                    ae.action.sector, ae.action.pathver, ae.service_info.identity, ae.authorized,
                );
            } else {
                let mut row = vec![
                    TableCell::new(ae.action.sector.clone()),
                    TableCell::new(ae.action.pathver.clone()),
                    TableCell::new(ae.service_info.identity.clone()),
                ];
                if *all {
                    row.push(TableCell::new(ae.authorized.to_string()));
                }
                let mut row = Row::new(row);
                if i > 1 {
                    row.has_separator = false;
                }
                table.add_row(row);
            }
        }
        if !raw {
            print!("{}", table.render());
            println!("{i} actions found offered by {} services", services.len());
        }
        Ok(())
    }
    fn list_services(&self, _config: &Config, registry: &ServiceRegistry) -> Result<()> {
        let ListCommand::Services {
            with_action,
            all,
            uri,
            name,
            raw,
            sector,
        } = self
        else {
            return Err(anyhow::anyhow!("Invalid command"));
        };

        let mut actions = 0usize;
        let mut table = Table::new();
        let mut headers = vec![
            TableCell::new("Service"),
            TableCell::new("Uri"),
            TableCell::new("Sectors"),
            TableCell::new("Actions"),
        ];
        if *all {
            headers.push(TableCell::new("Authorized"));
        }
        if !raw {
            table.add_row(Row::new(headers));
        }

        let mut i = 0;
        let mut unique_services = std::collections::HashMap::new();

        for ae in registry.actions_iter() {
            if let Some(sector) = &sector {
                if !ae.action.sector.contains(sector) {
                    continue;
                }
            }
            if let Some(with_action) = &with_action {
                if !ae.action.pathver.contains(with_action) {
                    continue;
                }
            }
            if let Some(uri) = &uri {
                if !ae.service_info.uri.contains(uri) {
                    continue;
                }
            }
            if let Some(name) = &name {
                if !ae.service_info.identity.starts_with(name) {
                    continue;
                }
            }
            if !all && !ae.authorized {
                continue;
            }
            actions += 1;

            let service_identity = ae.service_info.identity.clone();
            unique_services
                .entry(service_identity)
                .or_insert_with(|| {
                    (
                        ae.service_info.uri.clone(),
                        HashSet::new(),
                        0,
                        ae.authorized,
                    )
                })
                .1
                .insert(ae.action.sector.clone());
            unique_services
                .get_mut(&ae.service_info.identity)
                .unwrap()
                .2 += 1;
        }

        for (service_identity, (uri, sectors, action_count, authorized)) in unique_services {
            i += 1;

            if *raw {
                println!(
                    "{service_identity}\t{uri}\t{}\t{action_count}\t{authorized}",
                    sectors.into_iter().collect::<Vec<_>>().join(","),
                );
            } else {
                let mut row = vec![
                    TableCell::new(service_identity),
                    TableCell::new(uri),
                    TableCell::new(sectors.into_iter().collect::<Vec<_>>().join(",")),
                    TableCell::new(action_count.to_string()),
                ];
                row.push(TableCell::new(authorized.to_string()));
                let mut row = Row::new(row);
                if i > 1 {
                    row.has_separator = false;
                }
                table.add_row(row);
            }
        }

        if !raw {
            print!("{}", table.render());
            println!("{i} services found with {} actions", actions);
        }
        Ok(())
    }
    fn list_sectors(&self, _config: &Config, registry: &ServiceRegistry) -> Result<()> {
        let ListCommand::Sectors { raw } = self else {
            return Err(anyhow::anyhow!("Invalid command"));
        };
        let mut sectors: BTreeMap<String, (u32, HashSet<String>)> = BTreeMap::new();
        let mut services: HashSet<String> = HashSet::new();
        let mut actions = 0usize;

        for ae in registry.actions_iter() {
            let entry = sectors
                .entry(ae.action.sector.clone())
                .or_insert((0, HashSet::new()));
            entry.0 += 1; // Increment action count
            entry.1.insert(ae.service_info.identity.clone());
            services.insert(ae.service_info.identity.clone());
            actions += 1;
        }

        let mut table = Table::new();
        if !raw {
            table.add_row(Row::new(vec![
                TableCell::new("Sector"),
                TableCell::new("Actions"),
                TableCell::new("Services"),
            ]));
        }

        for (i, (sector, (actions, services))) in sectors.iter().enumerate() {
            if *raw {
                println!("{sector}\t{actions}\t{}", services.len());
            } else {
                let mut row = Row::new(vec![
                    TableCell::new(sector),
                    TableCell::new(actions.to_string()),
                    TableCell::new(services.len().to_string()),
                ]);
                if i > 0 {
                    row.has_separator = false;
                }
                table.add_row(row);
            }
        }

        print!("{}", table.render());
        println!(
            "{} sectors found with {} actions offered by {} services",
            sectors.len(),
            actions,
            services.len()
        );

        Ok(())
    }
}

use itertools::izip;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{from_value, Map, Value};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ServiceInfo {
    identity: String,
    weight: u32,
    interval: u32,
    uri: String,
    actions: Vec<Action>,
    timestamp: f64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Action {
    path: String, // Eg product.sku.fetch
    version: u32, // Eg 1
    flags: Vec<Flag>,
    sector: String,
    envelopes: Vec<String>,
    packet_section: PacketSection,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
enum Flag {
    NoAuth,
    Timeout(u32), // eg t600
    Other(String),
    CrudOp(CrudOp),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
enum CrudOp {
    Create,
    Read,
    Update,
    Delete,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
enum PacketSection {
    V3,
    V4,
}

#[derive(Debug)]
pub enum ServiceInfoParseError {
    ExpectedJsonArray,
    InvalidRootArray,
    MissingField(&'static str),
    InvalidField(&'static str),
    JsonError(serde_json::Error),
    RLEValue(&'static str, usize, serde_json::Error),
    RLEChunkLen(
        /// name
        &'static str,
        /// chunk number
        usize,
        /// erroneous length
        usize,
    ),
    RLERepeatCount(&'static str, usize),
    InvalidV3Namespace(usize),
    InvalidV3Action(usize, usize, &'static str),
}

/// display all branches
impl fmt::Display for ServiceInfoParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServiceInfoParseError::ExpectedJsonArray => write!(f, "Expected JSON array"),
            ServiceInfoParseError::InvalidRootArray => write!(f, "Invalid array length"),
            ServiceInfoParseError::MissingField(field) => write!(f, "Missing field: {}", field),
            ServiceInfoParseError::InvalidField(field) => write!(f, "Invalid field: {}", field),
            ServiceInfoParseError::JsonError(e) => write!(f, "JSON error: {}", e),
            ServiceInfoParseError::RLEValue(name, i, e) => {
                write!(f, "RLE value error: {} at {}: {}", e, name, i)
            }
            ServiceInfoParseError::RLEChunkLen(name, i, len) => {
                write!(f, "RLE chunk length error: {} at {}: {}", len, name, i)
            }
            ServiceInfoParseError::RLERepeatCount(name, i) => {
                write!(f, "RLE repeat count error: {} at {}: {}", i, name, i)
            }
            ServiceInfoParseError::InvalidV3Namespace(ns_i) => {
                write!(f, "Invalid v3 namespace at {}", ns_i)
            }
            ServiceInfoParseError::InvalidV3Action(ns_i, ac_i, reason) => {
                write!(
                    f,
                    "Invalid v3 action at namespace {} action {} {}",
                    ns_i, ac_i, reason
                )
            }
        }
    }
}

impl From<serde_json::Error> for ServiceInfoParseError {
    fn from(e: serde_json::Error) -> Self {
        ServiceInfoParseError::JsonError(e)
    }
}

impl ServiceInfo {
    pub fn parse(v: &str) -> Result<Self, ServiceInfoParseError> {
        let value: serde_json::Value = serde_json::from_str(v)?;

        let array = value
            .as_array()
            .ok_or_else(|| ServiceInfoParseError::ExpectedJsonArray)?;

        if array.len() != 9 {
            return Err(ServiceInfoParseError::InvalidRootArray);
        }

        let version = array[0]
            .as_u64()
            .ok_or_else(|| ServiceInfoParseError::MissingField("version"))?;

        let identity = array[1]
            .as_str()
            .ok_or_else(|| ServiceInfoParseError::MissingField("identity"))?;

        let v3_sector = array[2]
            .as_str()
            .ok_or_else(|| ServiceInfoParseError::MissingField("sector"))?
            .to_string();

        let weight = array[3]
            .as_u64()
            .ok_or_else(|| ServiceInfoParseError::MissingField("weight"))?
            as u32;

        let interval = array[4]
            .as_u64()
            .ok_or_else(|| ServiceInfoParseError::MissingField("interval"))?
            as u32;

        let uri = array[5]
            .as_str()
            .ok_or_else(|| ServiceInfoParseError::MissingField("uri"))?
            .to_string();

        let mut v3_envelopes: Vec<String> = Vec::new();

        let envelopes_and_v4actions = array[6]
            .as_array()
            .ok_or_else(|| ServiceInfoParseError::MissingField("envelopes_and_v4actions"))?;

        let v3_actions = array[7]
            .as_array()
            .ok_or_else(|| ServiceInfoParseError::MissingField("v3_actions"))?;

        let timestamp = array[8]
            .as_f64()
            .ok_or_else(|| ServiceInfoParseError::MissingField("timestamp"))?;

        let mut actions: Vec<Action> = Vec::new();
        // Iterate over the envelopes and v4 actions
        for value in envelopes_and_v4actions {
            match value {
                Value::String(envelope) => v3_envelopes.push(envelope.to_string()),
                Value::Object(obj) => parse_v4_actions(obj, &mut actions)?,
                _ => {}
            }
        }
        parse_v3_actions(v3_actions, &v3_sector, &v3_envelopes, &mut actions)?;

        Ok(ServiceInfo {
            identity: identity.to_string(),
            weight,
            interval,
            uri,
            actions,
            timestamp,
        })
    }
}

fn parse_v3_actions(
    obj: &Vec<Value>,
    sector: &str,
    envelopes: &[String],
    actions: &mut Vec<Action>,
) -> Result<(), ServiceInfoParseError> {
    // this is a much simpler format than v4
    // [
    //     [
    //         "API.Documentation",
    //         [ "fetch_tree", "noauth,read" ]
    //     ],
    //     [
    //         "API.Status",
    //         [ "health_check", "" ]
    //     ],
    //     ...
    // ]
    for (ns_i, ns) in obj.iter().enumerate() {
        match ns {
            Value::Array(ns_arr) if ns_arr.len() > 0 => {
                let namespace = ns_arr[0]
                    .as_str()
                    .ok_or(ServiceInfoParseError::MissingField("namespace [0]"))?;

                for (ac_i, ac) in ns_arr.iter().skip(1).enumerate() {
                    match ac {
                        Value::Array(ac_arr) if ac_arr.len() == 2 || ac_arr.len() == 3 => {
                            let name = ac_arr[0].as_str().ok_or(
                                ServiceInfoParseError::InvalidV3Action(ns_i, ac_i, "name"),
                            )?;

                            let flags = ac_arr[1].as_str().ok_or(
                                ServiceInfoParseError::InvalidV3Action(ns_i, ac_i, "flags"),
                            )?;

                            // optional, defaults to 1
                            // if specified it must parse to a number
                            let version = match ac_arr.get(2) {
                                Some(Value::Number(v)) => {
                                    v.as_u64().ok_or(ServiceInfoParseError::InvalidV3Action(
                                        ns_i,
                                        ac_i,
                                        "version must be a number",
                                    ))? as u32
                                }
                                _ => 1,
                            };

                            let action = Action {
                                path: format!("{}.{}", namespace, name),
                                version,
                                flags: flags
                                    .split(',')
                                    .filter(|s| !s.is_empty())
                                    .map(Flag::parse)
                                    .collect(),
                                sector: sector.to_string(),
                                envelopes: envelopes.to_vec(),
                                packet_section: PacketSection::V3,
                            };
                            actions.push(action);
                        }
                        _ => {
                            return Err(ServiceInfoParseError::InvalidV3Action(
                                ns_i,
                                ac_i,
                                "(not array of length 2",
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(ServiceInfoParseError::InvalidV3Namespace(ns_i));
            }
        }
    }
    Ok(())
}

fn parse_v4_actions(
    obj: &serde_json::Map<String, Value>,
    actions: &mut Vec<Action>,
) -> Result<(), ServiceInfoParseError> {
    // "acflag":["","t300",[11,"noauth"]],
    // "vmaj":4,
    // "acenv":[[2,"json,jsonstore,extdirect"],[11,"web"]],
    // "acsec":[[2,"background"],[11,"web"]],
    // "acns":[[2,"Channel.Aabaco.ImageInterchange"],[2,"Upload.Channel.Aabaco"],[8,"Upload.Channel.Shopify"],"Upload.Integration.GlobalE"],
    // "acname":["_evaluate","_execute","inventory_pull","order_push","app_customers_data_request","app_customers_redact","app_shop_redact","orders_create","orders_updated","products_create","products_delete","products_update","rma"],
    // "vmin":0,
    // "acver":[[13,1]]
    let namespaces = unrle::<String>(obj, "acns", true, 0)?;
    let names = unrle::<String>(obj, "acname", true, 0)?;
    let len = names.len();
    let envelopess = unrle::<String>(obj, "acenv", true, 0)?;
    let sectors = unrle::<String>(obj, "acsec", true, 0)?;
    let compats = unrle::<u32>(obj, "accompat", false, len)?;
    let acvers = unrle::<u32>(obj, "acver", false, len)?;
    // let vmins = unrle::<u32>(obj, "acver", false, len)?;
    let flagss = unrle::<String>(obj, "acflag", false, len)?;

    for (namespace, name, compat, ver, flags, envelopes, sector) in
        izip!(namespaces, names, compats, acvers, flagss, envelopess, sectors)
    {
        let action = Action {
            path: format!("{}.{}", namespace, name),
            version: ver,
            flags: flags
                .split(',')
                .filter(|s| !s.is_empty())
                .map(Flag::parse)
                .collect(),
            sector: sector.to_lowercase(),
            envelopes: envelopes
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
            packet_section: PacketSection::V4,
        };

        actions.push(action);

        //     if compatver
        //         .as_u64()
        //         .ok_or(ServiceInfoParseError::MissingField("compatver"))?
        //         != 1
        //     {
        //         continue;
        //     }

        //     let key = format!(
        //         "{}:{}.{}.v{}",
        //         info.sector, actns_str, action_str, actver_num
        //     );
        //     map.insert(key, info.clone());

        //     for tag in &info.flags {
        //         if let Flag::CrudOp(op) = tag {
        //             let alias_key =
        //                 format!("{}:{}._{:?}.v{}", info.sector, actns_str, op, actver_num);
        //             map.insert(alias_key, info.clone());
        //         }
        //     }
        // }
    }

    Ok(())
}

fn unrle<T>(
    obj: &Map<String, Value>,
    name: &'static str,
    required: bool,
    len: usize,
) -> Result<Vec<T>, ServiceInfoParseError>
where
    T: Default + Clone + DeserializeOwned,
{
    match obj.get(name) {
        None => {
            if required {
                return Err(ServiceInfoParseError::MissingField(name));
            } else {
                Ok((0..len).map(|_| T::default()).collect())
            }
        }
        // {"acflag":["","t300",[11,"noauth"]],
        // "vmaj":4,
        // "acenv":[[2,"json,jsonstore,extdirect"],[11,"web"]],
        // "acsec":[[2,"background"],[11,"web"]],
        // "acns":[[2,"Channel.Aabaco.ImageInterchange"],[2,"Upload.Channel.Aabaco"],[8,"Upload.Channel.Shopify"],"Upload.Integration.GlobalE"],
        // "acname":["_evaluate","_execute","inventory_pull","order_push","app_customers_data_request","app_customers_redact","app_shop_redact","orders_create","orders_updated","products_create","products_delete","products_update","rma"],
        // "vmin":0,
        // "acver":[[13,1]]}]
        Some(Value::Array(rle)) => {
            let mut out: Vec<T> = Vec::new();
            for (i, entry) in rle.iter().enumerate() {
                match entry {
                    Value::Array(arr) => {
                        if arr.len() == 2 {
                            let repeat = arr[0]
                                .as_u64()
                                .ok_or(ServiceInfoParseError::RLERepeatCount(name, i))?;

                            let value: T = from_value(arr[1].clone())
                                .map_err(|e| ServiceInfoParseError::RLEValue(name, i, e))?;

                            out.extend(std::iter::repeat(value).take(repeat as usize));
                        } else {
                            return Err(ServiceInfoParseError::RLEChunkLen(name, i, arr.len()));
                        }
                    }
                    _ => {
                        let value: T = from_value(entry.clone())
                            .map_err(|e| ServiceInfoParseError::RLEValue(name, i, e))?;

                        out.push(value);
                    }
                };
            }

            Ok(out)
        }
        Some(value) => {
            let value: T = from_value(value.clone())
                .map_err(|e| ServiceInfoParseError::RLEValue(name, 0, e))?;

            Ok(vec![value; len])
        }
    }
}

impl CrudOp {
    fn parse(v: &str) -> Option<Self> {
        match v {
            "create" => Some(CrudOp::Create),
            "read" => Some(CrudOp::Read),
            "update" => Some(CrudOp::Update),
            "delete" => Some(CrudOp::Delete),
            _ => None,
        }
    }
}

impl Flag {
    fn parse(v: &str) -> Self {
        static TIMEOUT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^t(\d+)$").unwrap());

        match v {
            "noauth" => Flag::NoAuth,
            _ => {
                if let Some(caps) = TIMEOUT_RE.captures(v) {
                    let timeout: u32 = caps[1].parse().unwrap();
                    Flag::Timeout(timeout)
                } else if let Some(crud) = CrudOp::parse(v) {
                    Flag::CrudOp(crud)
                } else {
                    Flag::Other(v.to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse() {
        let info = ServiceInfo::parse(include_str!(
            "../../samples/service_info_packet_v3_data.json"
        ))
        .unwrap();

        // Does it match our reference?
        assert_eq!(
            info,
            serde_json::from_str::<ServiceInfo>(include_str!(
                "../../samples/service_info_packet_v3_data_parsed.json"
            ))
            .unwrap()
        );
    }
}

// ServiceInfo {
//     identity: "mainapi:4HaM4TN5IVSLNfqhERfKvsVu",
//     weight: 1,
//     interval: 5000,
//     uri: "beepish+tls://172.18.0.7:30201",
//     actions: [
//         Action { path: "Download.Financials.journalentries", version: 1, flags: [NoAuth], sector: "web", envelopes: ["web"] },
//         Action { path: "Download.PO.csv", version: 1, flags: [NoAuth], sector: "web", envelopes: ["web"] },
//         Action { path: "Download.PO.pdf", version: 1, flags: [NoAuth], sector: "web", envelopes: ["web"] },
//         Action { path: "Flat.calculate", version: 1, flags: [], sector: "taxmodule", envelopes: ["json", "jsonstore", "extdirect"] },
//         Action { path: "TaxJar.calculate", version: 1, flags: [], sector: "taxmodule", envelopes: ["json", "jsonstore", "extdirect"] },
//         Action { path: "VAT.calculate", version: 1, flags: [], sector: "taxmodule", envelopes: ["json", "jsonstore", "extdirect"] }
//     ],
//     timestamp: 1720724094.61916
// }

//! V3 and V4 announcement action parsing, including RLE decoding.

use itertools::izip;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::de::DeserializeOwned;
use serde_json::{from_value, Map, Value};

use super::{Action, AnnouncementBody, AnnouncementParams, CrudOp, Flag, PacketSection, ServiceInfo, ServiceInfoParseError};

pub(super) fn parse_v3_actions(
    obj: &[Value],
    sector: &str,
    envelopes: &[String],
    actions: &mut Vec<Action>,
) -> Result<(), ServiceInfoParseError> {
    for (ns_i, ns) in obj.iter().enumerate() {
        match ns {
            Value::Array(ns_arr) if !ns_arr.is_empty() => {
                let namespace = ns_arr[0].as_str().ok_or(ServiceInfoParseError::MissingField("namespace [0]"))?;

                for (ac_i, ac) in ns_arr.iter().skip(1).enumerate() {
                    match ac {
                        Value::Array(ac_arr) if ac_arr.len() == 2 || ac_arr.len() == 3 => {
                            let name = ac_arr[0]
                                .as_str()
                                .ok_or(ServiceInfoParseError::InvalidV3Action(ns_i, ac_i, "name"))?;
                            let flags = ac_arr[1]
                                .as_str()
                                .ok_or(ServiceInfoParseError::InvalidV3Action(ns_i, ac_i, "flags"))?;
                            let version = match ac_arr.get(2) {
                                Some(Value::Number(v)) => {
                                    v.as_u64()
                                        .ok_or(ServiceInfoParseError::InvalidV3Action(ns_i, ac_i, "version must be number"))?
                                        as u32
                                }
                                _ => 1,
                            };

                            let path = format!("{}.{}", namespace, name).to_lowercase().replace('/', ".");
                            let pathver = format!("{}~{}", path, version);
                            actions.push(Action {
                                path,
                                version,
                                pathver,
                                flags: flags.split(',').filter(|s| !s.is_empty()).map(parse_flag).collect(),
                                sector: sector.to_string(),
                                envelopes: envelopes.to_vec(),
                                packet_section: PacketSection::V3,
                            });
                        }
                        _ => {
                            return Err(ServiceInfoParseError::InvalidV3Action(ns_i, ac_i, "not array of length 2-3"));
                        }
                    }
                }
            }
            _ => return Err(ServiceInfoParseError::InvalidV3Namespace(ns_i)),
        }
    }
    Ok(())
}

pub(super) fn parse_v4_actions(obj: &Map<String, Value>, actions: &mut Vec<Action>) -> Result<(), ServiceInfoParseError> {
    let namespaces = unrle::<String>(obj, "acns", true, 0)?;
    let names = unrle::<String>(obj, "acname", true, 0)?;
    let len = names.len();
    let envelopess = unrle::<String>(obj, "acenv", true, 0)?;
    let sectors = unrle::<String>(obj, "acsec", true, 0)?;
    let compats = unrle::<u32>(obj, "accompat", false, len)?;
    let acvers = unrle::<u32>(obj, "acver", false, len)?;
    let flagss = unrle::<String>(obj, "acflag", false, len)?;

    for (namespace, name, compat, ver, flags, envelopes, sector) in izip!(namespaces, names, compats, acvers, flagss, envelopess, sectors) {
        // Perl ServiceInfo.pm:220, JS service.js:109
        if compat != 1 {
            continue;
        }

        let path = format!("{}.{}", namespace, name).to_lowercase().replace('/', ".");
        let pathver = format!("{}~{}", path, ver);
        actions.push(Action {
            path,
            version: ver,
            pathver,
            flags: flags.split(',').filter(|s| !s.is_empty()).map(parse_flag).collect(),
            sector: sector.to_lowercase(),
            envelopes: envelopes.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect(),
            packet_section: PacketSection::V4,
        });
    }
    Ok(())
}

pub(super) fn unrle<T>(obj: &Map<String, Value>, name: &'static str, required: bool, len: usize) -> Result<Vec<T>, ServiceInfoParseError>
where
    T: Default + Clone + DeserializeOwned,
{
    match obj.get(name) {
        None => {
            if required {
                Err(ServiceInfoParseError::MissingField(name))
            } else {
                Ok((0..len).map(|_| T::default()).collect())
            }
        }
        Some(Value::Array(rle)) => {
            let mut out: Vec<T> = Vec::new();
            for (i, entry) in rle.iter().enumerate() {
                match entry {
                    Value::Array(arr) if arr.len() == 2 => {
                        let repeat = arr[0].as_u64().ok_or(ServiceInfoParseError::RLERepeatCount(name, i))?;
                        let value: T = from_value(arr[1].clone()).map_err(|e| ServiceInfoParseError::RLEValue(name, i, e))?;
                        out.extend(std::iter::repeat_n(value, repeat as usize));
                    }
                    Value::Array(arr) => return Err(ServiceInfoParseError::RLEChunkLen(name, i, arr.len())),
                    _ => {
                        let value: T = from_value(entry.clone()).map_err(|e| ServiceInfoParseError::RLEValue(name, i, e))?;
                        out.push(value);
                    }
                }
            }
            Ok(out)
        }
        Some(value) => {
            let value: T = from_value(value.clone()).map_err(|e| ServiceInfoParseError::RLEValue(name, 0, e))?;
            Ok(vec![value; len])
        }
    }
}

/// Parse an announcement JSON blob into an AnnouncementBody.
/// F2: Moved from mod.rs — parsing logic belongs in parse.rs.
pub(super) fn parse_announcement_body(v: &str) -> Result<AnnouncementBody, ServiceInfoParseError> {
    let value: Value = serde_json::from_str(v)?;
    let array = value.as_array().ok_or(ServiceInfoParseError::ExpectedJsonArray)?;
    if array.len() != 9 {
        return Err(ServiceInfoParseError::InvalidRootArray);
    }

    let version = array[0].as_u64().ok_or(ServiceInfoParseError::MissingField("version"))?;
    if version != 3 {
        return Err(ServiceInfoParseError::InvalidField("version"));
    }

    let identity = array[1]
        .as_str()
        .ok_or(ServiceInfoParseError::MissingField("identity"))?
        .to_string();
    let v3_sector = array[2].as_str().ok_or(ServiceInfoParseError::MissingField("sector"))?.to_string();
    let weight = array[3].as_u64().ok_or(ServiceInfoParseError::MissingField("weight"))? as u32;
    let interval = array[4].as_u64().ok_or(ServiceInfoParseError::MissingField("interval"))? as u32;
    let uri = array[5].as_str().ok_or(ServiceInfoParseError::MissingField("uri"))?.to_string();

    let envelopes_and_v4 = array[6]
        .as_array()
        .ok_or(ServiceInfoParseError::MissingField("envelopes_and_v4actions"))?;
    let v3_actions = array[7].as_array().ok_or(ServiceInfoParseError::MissingField("v3_actions"))?;
    let timestamp = array[8].as_f64().ok_or(ServiceInfoParseError::MissingField("timestamp"))?;

    let mut v3_envelopes: Vec<String> = Vec::new();
    let mut actions: Vec<Action> = Vec::new();

    for value in envelopes_and_v4 {
        match value {
            Value::String(envelope) => v3_envelopes.push(envelope.to_string()),
            Value::Object(obj) => parse_v4_actions(obj, &mut actions)?,
            _ => {}
        }
    }
    parse_v3_actions(v3_actions, &v3_sector, &v3_envelopes, &mut actions)?;

    Ok(AnnouncementBody {
        info: ServiceInfo {
            identity,
            uri,
            fingerprint: None,
        },
        params: AnnouncementParams {
            weight,
            interval,
            timestamp,
        },
        actions,
    })
}

/// Parse a flag string into a Flag enum value.
pub(super) fn parse_flag(v: &str) -> Flag {
    static TIMEOUT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^t(\d+)$").unwrap());
    match v {
        "noauth" => Flag::NoAuth,
        _ => {
            if let Some(caps) = TIMEOUT_RE.captures(v) {
                Flag::Timeout(caps[1].parse().unwrap())
            } else if let Some(crud) = parse_crud_op(v) {
                Flag::CrudOp(crud)
            } else {
                Flag::Other(v.to_string())
            }
        }
    }
}

/// Parse a CRUD operation string.
pub(super) fn parse_crud_op(v: &str) -> Option<CrudOp> {
    match v {
        "create" => Some(CrudOp::Create),
        "read" => Some(CrudOp::Read),
        "update" => Some(CrudOp::Update),
        "destroy" => Some(CrudOp::Delete),
        _ => None,
    }
}

//! V3 and V4 announcement action parsing, including RLE decoding.

use itertools::izip;
use serde::de::DeserializeOwned;
use serde_json::{from_value, Map, Value};

use super::{Action, Flag, PacketSection, ServiceInfoParseError};

pub(super) fn parse_v3_actions(
    obj: &[Value],
    sector: &str,
    envelopes: &[String],
    actions: &mut Vec<Action>,
) -> Result<(), ServiceInfoParseError> {
    for (ns_i, ns) in obj.iter().enumerate() {
        match ns {
            Value::Array(ns_arr) if !ns_arr.is_empty() => {
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
                            let version = match ac_arr.get(2) {
                                Some(Value::Number(v)) => {
                                    v.as_u64().ok_or(ServiceInfoParseError::InvalidV3Action(
                                        ns_i,
                                        ac_i,
                                        "version must be number",
                                    ))? as u32
                                }
                                _ => 1,
                            };

                            let path = format!("{}.{}", namespace, name)
                                .to_lowercase()
                                .replace('/', ".");
                            let pathver = format!("{}~{}", path, version);
                            actions.push(Action {
                                path,
                                version,
                                pathver,
                                flags: flags
                                    .split(',')
                                    .filter(|s| !s.is_empty())
                                    .map(Flag::parse_str)
                                    .collect(),
                                sector: sector.to_string(),
                                envelopes: envelopes.to_vec(),
                                packet_section: PacketSection::V3,
                            });
                        }
                        _ => {
                            return Err(ServiceInfoParseError::InvalidV3Action(
                                ns_i,
                                ac_i,
                                "not array of length 2-3",
                            ));
                        }
                    }
                }
            }
            _ => return Err(ServiceInfoParseError::InvalidV3Namespace(ns_i)),
        }
    }
    Ok(())
}

pub(super) fn parse_v4_actions(
    obj: &Map<String, Value>,
    actions: &mut Vec<Action>,
) -> Result<(), ServiceInfoParseError> {
    let namespaces = unrle::<String>(obj, "acns", true, 0)?;
    let names = unrle::<String>(obj, "acname", true, 0)?;
    let len = names.len();
    let envelopess = unrle::<String>(obj, "acenv", true, 0)?;
    let sectors = unrle::<String>(obj, "acsec", true, 0)?;
    let compats = unrle::<u32>(obj, "accompat", false, len)?;
    let acvers = unrle::<u32>(obj, "acver", false, len)?;
    let flagss = unrle::<String>(obj, "acflag", false, len)?;

    for (namespace, name, compat, ver, flags, envelopes, sector) in
        izip!(namespaces, names, compats, acvers, flagss, envelopess, sectors)
    {
        // Perl ServiceInfo.pm:220, JS service.js:109
        if compat != 1 {
            continue;
        }

        let path = format!("{}.{}", namespace, name)
            .to_lowercase()
            .replace('/', ".");
        let pathver = format!("{}~{}", path, ver);
        actions.push(Action {
            path,
            version: ver,
            pathver,
            flags: flags
                .split(',')
                .filter(|s| !s.is_empty())
                .map(Flag::parse_str)
                .collect(),
            sector: sector.to_lowercase(),
            envelopes: envelopes
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
            packet_section: PacketSection::V4,
        });
    }
    Ok(())
}

pub(super) fn unrle<T>(
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
                        let repeat = arr[0]
                            .as_u64()
                            .ok_or(ServiceInfoParseError::RLERepeatCount(name, i))?;
                        let value: T = from_value(arr[1].clone())
                            .map_err(|e| ServiceInfoParseError::RLEValue(name, i, e))?;
                        out.extend(std::iter::repeat_n(value, repeat as usize));
                    }
                    Value::Array(arr) => {
                        return Err(ServiceInfoParseError::RLEChunkLen(name, i, arr.len()))
                    }
                    _ => {
                        let value: T = from_value(entry.clone())
                            .map_err(|e| ServiceInfoParseError::RLEValue(name, i, e))?;
                        out.push(value);
                    }
                }
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

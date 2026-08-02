use serde_json::Value;

use crate::{
    FilesystemUsage, InfraError, InfraResult, MountInfo, NetworkAddress, NetworkInterface,
    PortInfo, ServiceStatus,
};

pub(crate) fn parse_services(raw: &str) -> Vec<ServiceStatus> {
    raw.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let unit = fields.next()?.to_owned();
            let load = fields.next()?.to_owned();
            let active = fields.next()?.to_owned();
            let sub = fields.next()?.to_owned();
            let description = fields.collect::<Vec<_>>().join(" ");
            Some(ServiceStatus {
                unit,
                load,
                active,
                sub,
                description,
            })
        })
        .collect()
}

pub(crate) fn parse_network(raw: &str) -> InfraResult<Vec<NetworkInterface>> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| parse_error(format!("invalid ip JSON: {error}")))?;
    let rows = value
        .as_array()
        .ok_or_else(|| parse_error("ip JSON root is not an array"))?;
    rows.iter()
        .map(|row| {
            let addresses = row
                .get("addr_info")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            Some(NetworkAddress {
                                family: item.get("family")?.as_str()?.to_owned(),
                                address: item.get("local")?.as_str()?.to_owned(),
                                prefix_len: u8::try_from(item.get("prefixlen")?.as_u64()?).ok()?,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(NetworkInterface {
                index: row
                    .get("ifindex")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                name: row
                    .get("ifname")
                    .and_then(Value::as_str)
                    .ok_or_else(|| parse_error("interface has no name"))?
                    .to_owned(),
                state: row
                    .get("operstate")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                mtu: row.get("mtu").and_then(Value::as_u64),
                addresses,
            })
        })
        .collect()
}

pub(crate) fn parse_mounts(raw: &str) -> InfraResult<Vec<MountInfo>> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| parse_error(format!("invalid findmnt JSON: {error}")))?;
    let rows = value
        .get("filesystems")
        .and_then(Value::as_array)
        .ok_or_else(|| parse_error("findmnt JSON has no filesystems"))?;
    let mut mounts = Vec::new();
    for row in rows {
        collect_mount(row, &mut mounts)?;
    }
    Ok(mounts)
}

fn collect_mount(row: &Value, mounts: &mut Vec<MountInfo>) -> InfraResult<()> {
    mounts.push(MountInfo {
        target: string(row, &["target"]).ok_or_else(|| parse_error("mount has no target"))?,
        source: string(row, &["source"]),
        filesystem: string(row, &["fstype"]),
        options: string(row, &["options"]),
        size_bytes: unsigned(row, &["size"]),
        used_bytes: unsigned(row, &["used"]),
        available_bytes: unsigned(row, &["avail"]),
    });
    if let Some(children) = row.get("children").and_then(Value::as_array) {
        for child in children {
            collect_mount(child, mounts)?;
        }
    }
    Ok(())
}

pub(crate) fn parse_ports(raw: &str) -> Vec<PortInfo> {
    raw.lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 6 {
                return None;
            }
            Some(PortInfo {
                protocol: fields[0].to_owned(),
                state: fields[1].to_owned(),
                local_address: fields[4].to_owned(),
                peer_address: fields[5].to_owned(),
                process: (fields.len() > 6).then(|| fields[6..].join(" ")),
            })
        })
        .collect()
}

pub(crate) fn parse_usage(raw: &str) -> InfraResult<FilesystemUsage> {
    let row = raw
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .ok_or_else(|| parse_error("df returned no rows"))?;
    let fields = row.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 7 {
        return Err(parse_error("df row has fewer than seven columns"));
    }
    Ok(FilesystemUsage {
        source: fields[0].to_owned(),
        filesystem: fields[1].to_owned(),
        size_bytes: parse_u64(fields[2], "size")?,
        used_bytes: parse_u64(fields[3], "used")?,
        available_bytes: parse_u64(fields[4], "available")?,
        usage_percent: fields[5]
            .trim_end_matches('%')
            .parse()
            .map_err(|_| parse_error("invalid usage percentage"))?,
        target: fields[6..].join(" "),
    })
}

fn string(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str).map(str::to_owned))
}

fn unsigned(value: &Value, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        let value = value.get(*name)?;
        value.as_u64().or_else(|| value.as_str()?.parse().ok())
    })
}

fn parse_u64(value: &str, field: &str) -> InfraResult<u64> {
    value
        .parse()
        .map_err(|_| parse_error(format!("invalid {field} byte count")))
}

fn parse_error(message: impl Into<String>) -> InfraError {
    InfraError::Parse {
        domain: "host",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "host_system_parse_tests.rs"]
mod tests;

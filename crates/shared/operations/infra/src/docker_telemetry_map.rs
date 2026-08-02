use serde_json::Value;
use soma_fleet::HostRecord;

use crate::{ContainerStatsSnapshot, DockerDiskUsage, DockerUsageCategory, InfraResult};

pub(crate) fn map_disk_usage(host: &HostRecord, value: &Value) -> InfraResult<DockerDiskUsage> {
    Ok(DockerDiskUsage {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        layers_size_bytes: unsigned(value, &["LayersSize", "layers_size"]),
        images: category(value, &["Images", "images"], |item| {
            unsigned(item, &["Size", "size"])
        }),
        containers: category(value, &["Containers", "containers"], |item| {
            unsigned(item, &["SizeRw", "size_rw"])
        }),
        volumes: category(value, &["Volumes", "volumes"], |item| {
            nested_unsigned(item, &[&["UsageData", "usage_data"], &["Size", "size"]])
        }),
        build_cache: category(value, &["BuildCache", "build_cache"], |item| {
            unsigned(item, &["Size", "size"])
        }),
    })
}

pub(crate) fn map_container_stats(
    host: &HostRecord,
    container: &str,
    value: &Value,
) -> InfraResult<ContainerStatsSnapshot> {
    let (network_rx_bytes, network_tx_bytes) = network_totals(value);
    let (block_read_bytes, block_write_bytes) = block_totals(value);
    Ok(ContainerStatsSnapshot {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        container: container.to_owned(),
        read_at: string(value, &["read", "Read"]),
        pids_current: nested_unsigned(
            value,
            &[&["pids_stats", "PidsStats"], &["current", "Current"]],
        ),
        memory_usage_bytes: nested_unsigned(
            value,
            &[&["memory_stats", "MemoryStats"], &["usage", "Usage"]],
        ),
        memory_limit_bytes: nested_unsigned(
            value,
            &[&["memory_stats", "MemoryStats"], &["limit", "Limit"]],
        ),
        cpu_total_usage: nested_unsigned(
            value,
            &[
                &["cpu_stats", "CpuStats"],
                &["cpu_usage", "CpuUsage"],
                &["total_usage", "TotalUsage"],
            ],
        ),
        system_cpu_usage: nested_unsigned(
            value,
            &[
                &["cpu_stats", "CpuStats"],
                &["system_cpu_usage", "SystemCpuUsage"],
            ],
        ),
        online_cpus: nested_unsigned(
            value,
            &[&["cpu_stats", "CpuStats"], &["online_cpus", "OnlineCpus"]],
        ),
        network_rx_bytes,
        network_tx_bytes,
        block_read_bytes,
        block_write_bytes,
    })
}

fn category(value: &Value, names: &[&str], size: impl Fn(&Value) -> u64) -> DockerUsageCategory {
    let values = field(value, names)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    DockerUsageCategory {
        count: values.len() as u64,
        size_bytes: values.iter().map(size).sum(),
    }
}

fn network_totals(value: &Value) -> (u64, u64) {
    field(value, &["networks", "Networks"])
        .and_then(Value::as_object)
        .map(|networks| {
            networks.values().fold((0_u64, 0_u64), |(rx, tx), network| {
                (
                    rx.saturating_add(unsigned(network, &["rx_bytes", "RxBytes"])),
                    tx.saturating_add(unsigned(network, &["tx_bytes", "TxBytes"])),
                )
            })
        })
        .unwrap_or_default()
}

fn block_totals(value: &Value) -> (u64, u64) {
    let rows = nested(
        value,
        &[
            &["blkio_stats", "BlkioStats"],
            &["io_service_bytes_recursive", "IoServiceBytesRecursive"],
        ],
    )
    .and_then(Value::as_array)
    .map(Vec::as_slice)
    .unwrap_or_default();
    rows.iter().fold((0_u64, 0_u64), |(read, write), row| {
        let operation = string(row, &["op", "Op"])
            .unwrap_or_default()
            .to_ascii_lowercase();
        let bytes = unsigned(row, &["value", "Value"]);
        match operation.as_str() {
            "read" => (read.saturating_add(bytes), write),
            "write" => (read, write.saturating_add(bytes)),
            _ => (read, write),
        }
    })
}

fn nested_unsigned(value: &Value, path: &[&[&str]]) -> u64 {
    nested(value, path)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
        })
        .unwrap_or_default()
}

fn nested<'a>(mut value: &'a Value, path: &[&[&str]]) -> Option<&'a Value> {
    for names in path {
        value = field(value, names)?;
    }
    Some(value)
}

fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    names.iter().find_map(|name| object.get(*name))
}

fn string(value: &Value, names: &[&str]) -> Option<String> {
    field(value, names)?.as_str().map(str::to_owned)
}

fn unsigned(value: &Value, names: &[&str]) -> u64 {
    field(value, names)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "docker_telemetry_map_tests.rs"]
mod tests;

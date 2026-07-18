//! Reusable MCP proxy route projection helpers.

use std::collections::{BTreeMap, BTreeSet};

use soma_mcp_client::upstream::{PromptDescriptor, ResourceDescriptor, ToolDescriptor};

mod projection;

pub use projection::{rmcp_prompt_from_route, rmcp_resource_from_route, rmcp_tool_from_route};

const DEFAULT_UPSTREAM_RESOURCE_PREFIX: &str = "mcp-gateway://upstream/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolRoute {
    pub name: String,
    pub upstream: String,
    pub native_name: String,
    pub descriptor: ToolDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceRoute {
    pub uri: String,
    pub upstream: String,
    pub native_uri: String,
    pub descriptor: ResourceDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptRoute {
    pub name: String,
    pub upstream: String,
    pub native_name: String,
    pub descriptor: PromptDescriptor,
}

pub fn tool_routes_from_candidates(
    candidates: Vec<(String, ToolDescriptor)>,
    reserved_names: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<McpToolRoute> {
    let counts = name_counts(candidates.iter().map(|(_, descriptor)| &descriptor.name));
    let reserved = reserved_names
        .into_iter()
        .map(|name| name.as_ref().to_owned())
        .collect::<BTreeSet<_>>();
    let mut used = BTreeSet::new();
    candidates
        .into_iter()
        .map(|(upstream, descriptor)| {
            let native_name = descriptor.name.clone();
            let preferred = if counts.get(native_name.as_str()) == Some(&1)
                && !reserved.contains(&native_name)
            {
                native_name.clone()
            } else {
                format!(
                    "{}__{}",
                    route_segment(&upstream),
                    route_segment(&native_name)
                )
            };
            McpToolRoute {
                name: unique_route_name(preferred, &mut used),
                upstream,
                native_name,
                descriptor,
            }
        })
        .collect()
}

pub fn resource_route(upstream: &str, descriptor: ResourceDescriptor) -> McpResourceRoute {
    McpResourceRoute {
        uri: upstream_resource_uri(upstream, &descriptor.uri),
        upstream: upstream.to_owned(),
        native_uri: descriptor.uri.clone(),
        descriptor,
    }
}

pub fn prompt_routes_from_candidates(
    candidates: Vec<(String, PromptDescriptor)>,
) -> Vec<McpPromptRoute> {
    let counts = name_counts(candidates.iter().map(|(_, descriptor)| &descriptor.name));
    let mut used = BTreeSet::new();
    candidates
        .into_iter()
        .map(|(upstream, descriptor)| {
            let native_name = descriptor.name.clone();
            let preferred = if counts.get(native_name.as_str()) == Some(&1) {
                native_name.clone()
            } else {
                format!(
                    "{}__{}",
                    route_segment(&upstream),
                    route_segment(&native_name)
                )
            };
            McpPromptRoute {
                name: unique_route_name(preferred, &mut used),
                upstream,
                native_name,
                descriptor,
            }
        })
        .collect()
}

pub fn upstream_resource_uri(upstream: &str, native_uri: &str) -> String {
    format!(
        "{DEFAULT_UPSTREAM_RESOURCE_PREFIX}{upstream}/{}",
        percent_encode(native_uri.as_bytes())
    )
}

pub fn parse_upstream_resource_uri(uri: &str) -> Option<(String, String)> {
    let rest = uri.strip_prefix(DEFAULT_UPSTREAM_RESOURCE_PREFIX)?;
    let (upstream, encoded) = rest.split_once('/')?;
    let native = percent_decode(encoded).ok()?;
    Some((upstream.to_owned(), native))
}

fn name_counts<'a>(names: impl Iterator<Item = &'a String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for name in names {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }
    counts
}

fn route_segment(value: &str) -> String {
    let routed: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if routed.is_empty() {
        "route".to_owned()
    } else {
        routed
    }
}

fn unique_route_name(preferred: String, used: &mut BTreeSet<String>) -> String {
    if used.insert(preferred.clone()) {
        return preferred;
    }
    for index in 2usize.. {
        let candidate = format!("{preferred}_{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("unbounded route suffix loop should always return")
}

fn percent_decode(value: &str) -> Result<String, ()> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = bytes.get(index + 1).copied().ok_or(())?;
            let lo = bytes.get(index + 2).copied().ok_or(())?;
            decoded.push(from_hex(hi)? << 4 | from_hex(lo)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ())
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut encoded = String::new();
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn from_hex(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}

/// Crate version from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

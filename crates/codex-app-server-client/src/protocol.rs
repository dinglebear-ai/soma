//! Generated types for the Codex app-server v2 JSON-RPC protocol.
//!
//! Everything in this module is generated at build time by `build.rs` (via
//! `typify`) from `schema/protocol.schema.json`. See the crate README for how
//! that schema was derived and how to regenerate it against a newer `codex`
//! CLI version.
#![allow(
    clippy::all,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    missing_docs,
    rustdoc::broken_intra_doc_links,
    rustdoc::bare_urls
)]

include!(concat!(env!("OUT_DIR"), "/protocol_generated.rs"));

#[cfg(test)]
mod tests {
    use super::RequestId;
    use std::collections::HashMap;

    #[test]
    fn request_id_is_eq_and_hash() {
        assert_eq!(RequestId::Int64(5), RequestId::Int64(5));
        assert_ne!(RequestId::Int64(5), RequestId::Int64(6));
        // Different variants are never equal, even when their string forms
        // coincide - `RequestId` is untagged, but the derived `PartialEq`
        // still discriminates by variant.
        assert_ne!(RequestId::Int64(5), RequestId::String("5".to_string()));

        // The real motivating use case: keying a map by `RequestId` to
        // correlate in-flight server->client requests (e.g. tracking
        // pending approval/elicitation requests by their app-server-assigned
        // id in a UI layer, or a multiplexing wrapper keying per-connection
        // state) without a caller-side newtype wrapper.
        let mut pending: HashMap<RequestId, &'static str> = HashMap::new();
        pending.insert(RequestId::Int64(1), "first");
        pending.insert(RequestId::String("abc".to_string()), "second");

        assert_eq!(pending.get(&RequestId::Int64(1)), Some(&"first"));
        assert_eq!(
            pending.get(&RequestId::String("abc".to_string())),
            Some(&"second")
        );
        assert_eq!(pending.get(&RequestId::Int64(2)), None);
    }
}

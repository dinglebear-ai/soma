//! `cargo xtask codex-schema bisect <dir>` - binary-searches a fresh schema
//! dump for the minimal definition(s) that panic typify's schema-merge
//! logic (typify-impl-0.7.0's `merge.rs:427`, "not yet implemented" - the
//! same failure mode `McpServerElicitationRequestParams` hit; see
//! `crates/codex-app-server-client/README.md`). Automates the "opaque out
//! half the new/changed definitions, see if the panic goes away" process
//! that was previously done by hand.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Map, Value};

use super::typify_probe::{self, ProbeOutcome};
use super::{load_combined_defs, merge, parse_gen_dir, PROTOCOL_SCHEMA_PATH};

pub fn run(args: &[String]) -> Result<()> {
    let gen_dir = parse_args(args)?;
    bisect(&gen_dir)
}

fn parse_args(args: &[String]) -> Result<PathBuf> {
    parse_gen_dir(
        args,
        "Usage: cargo xtask codex-schema bisect <path-to-codex-generate-json-schema-output-dir>",
    )
}

pub fn bisect(gen_dir: &Path) -> Result<()> {
    let (combined, defs) = load_combined_defs(gen_dir)?;

    println!(
        "==> probing the full merged schema ({} definitions)...",
        defs.len()
    );
    let full_outcome = typify_probe::probe(&combined);
    println!("    {}", full_outcome.summary());

    if !full_outcome.reproduces_target() {
        println!(
            "\n==> the full merged schema does not panic with the target typify merge.rs shape."
        );
        match full_outcome {
            ProbeOutcome::Success => {
                println!("    typify converted it successfully - nothing to bisect.")
            }
            other => println!(
                "    outcome was instead: {}\n    this tool only bisects the specific merge.rs \
                 \"not yet implemented\" panic - investigate this outcome by hand.",
                other.summary()
            ),
        }
        return Ok(());
    }

    let universe = suspect_universe(&defs);
    let scope_note = if universe.len() == defs.len() {
        " (no usable baseline diff - searching every definition)".to_string()
    } else {
        format!(" (definitions new or changed vs. the committed {PROTOCOL_SCHEMA_PATH})")
    };
    println!(
        "\n==> search universe: {} suspect definition(s){scope_note}",
        universe.len()
    );

    let culprits = minimize(&defs, universe);

    println!("\n==> bisection complete. culprit definition(s):");
    for name in &culprits {
        println!("\n--- {name} ---");
        match defs.get(name) {
            Some(schema) => println!(
                "{}",
                serde_json::to_string_pretty(schema).unwrap_or_else(|_| "<unserializable>".into())
            ),
            None => println!("<definition not found - internal bisection bug>"),
        }
    }
    println!(
        "\nNext step: either flatten the offending shape the way \
         McpServerElicitationRequestParams was handled (see \
         `xtask/src/codex_schema/merge.rs::flatten_base_plus_oneof` and the crate README's \"How \
         the typed protocol layer is built\" section), or, as a last resort, opaque this \
         definition to `true` (serde_json::Value) in `build_combined` and document the loss of \
         type fidelity."
    );
    Ok(())
}

/// Definitions that are new or textually changed vs. the currently
/// committed `protocol.schema.json`. Falls back to searching every
/// definition when no usable baseline is found (e.g. the baseline file is
/// missing/unparseable, or nothing actually changed).
fn suspect_universe(defs: &Map<String, Value>) -> BTreeSet<String> {
    let baseline_defs = std::fs::read_to_string(PROTOCOL_SCHEMA_PATH)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.get("definitions").and_then(Value::as_object).cloned());

    let Some(baseline_defs) = baseline_defs else {
        return defs.keys().cloned().collect();
    };

    let changed: BTreeSet<String> = defs
        .iter()
        .filter(|(name, schema)| baseline_defs.get(*name) != Some(schema))
        .map(|(name, _)| name.clone())
        .collect();

    if changed.is_empty() {
        defs.keys().cloned().collect()
    } else {
        changed
    }
}

/// Binary-searches `universe` (a set of definition names that, kept real
/// with everything else in `all_defs` at its natural realness, is already
/// known to reproduce the target panic) down to a minimal culprit set.
fn minimize(all_defs: &Map<String, Value>, mut universe: BTreeSet<String>) -> Vec<String> {
    loop {
        if universe.len() <= 1 {
            return universe.into_iter().collect();
        }

        let mid = universe.len() / 2;
        let (first, second) = split_at(&universe, mid);

        println!(
            "==> {} candidate(s) remaining - trying first half alone ({} defs)...",
            universe.len(),
            first.len()
        );
        if probe_with_kept(all_defs, &universe, &first).reproduces_target() {
            universe = first;
            continue;
        }

        println!(
            "    not reproduced; trying second half alone ({} defs)...",
            second.len()
        );
        if probe_with_kept(all_defs, &universe, &second).reproduces_target() {
            universe = second;
            continue;
        }

        println!(
            "    neither half alone reproduces the panic - these {} definition(s) appear to \
             jointly trigger it. Reporting the full remaining set.",
            universe.len()
        );
        return universe.into_iter().collect();
    }
}

fn split_at(set: &BTreeSet<String>, mid: usize) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut first = BTreeSet::new();
    let mut second = BTreeSet::new();
    for (i, item) in set.iter().enumerate() {
        if i < mid {
            first.insert(item.clone());
        } else {
            second.insert(item.clone());
        }
    }
    (first, second)
}

/// Builds a candidate schema where every definition in `universe` but not
/// in `keep_real` is opaqued to `true`; definitions outside `universe` are
/// left exactly as they are in `all_defs`. Probes it with typify.
fn probe_with_kept(
    all_defs: &Map<String, Value>,
    universe: &BTreeSet<String>,
    keep_real: &BTreeSet<String>,
) -> ProbeOutcome {
    let mut candidate_defs = all_defs.clone();
    for name in universe {
        if !keep_real.contains(name) {
            candidate_defs.insert(name.clone(), Value::Bool(true));
        }
    }
    typify_probe::probe(&merge::wrap_definitions(candidate_defs))
}

#[cfg(test)]
#[path = "bisect_tests.rs"]
mod tests;

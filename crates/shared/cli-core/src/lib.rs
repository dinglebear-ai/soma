//! Reusable CLI plumbing: output-format selection, JSON rendering,
//! confirmation I/O, and terminal/color capability policy.
//!
//! This crate is transport- and product-neutral. It has no knowledge of
//! Soma commands, action names, scopes, or exit-code policy — a consuming
//! CLI supplies its own parser and wording and calls into these primitives
//! for the generic mechanics.
//!
//! Modules here are extracted because `soma-cli` (or another consumer)
//! actually calls them. Table rendering, progress reporting, shell
//! completion generation, and structured CLI error presentation are not
//! part of this crate yet — add them here, backed by a real caller, when a
//! consumer actually needs them rather than speculatively ahead of use.

#![forbid(unsafe_code)]

pub mod color;
pub mod common_args;
pub mod confirmation;
pub mod json;
pub mod output;
pub mod terminal;

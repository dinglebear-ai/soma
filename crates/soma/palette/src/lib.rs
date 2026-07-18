//! `soma-palette` — Soma's Palette product API and adapter.
//!
//! Owns the `/v1/palette/*` route/handler definitions ([`router`]), the
//! Palette DTOs shared by the HTTP server and the desktop app, the product
//! mapping from provider `ToolSpec` Palette overlays into launcher actions,
//! product launcher execution/auth policy, product error mapping, and
//! product OpenAPI route metadata. Does not own Tauri window/tray/shortcut
//! mechanics (see `soma-tauri-shell`), `tauri.conf.json`/bundle metadata, or
//! frontend code — those stay in `apps/palette`.
//!
//! As of this crate's introduction (PR 17), [`router()`] is not yet mounted
//! into `apps/soma`'s HTTP server — that wiring lands in a follow-up PR. This
//! crate is buildable and independently tested (see `router_tests.rs`) ahead
//! of that mount.
//!
//! See `soma-architecture-refactor-plan-v3.md` section 3.25 and "PR 17".

pub mod auth;
pub mod catalog;
pub mod dto;
pub mod error;
pub mod execute;
pub mod openapi;
pub mod router;
pub mod schema;
pub mod search;
pub mod state;

pub use dto::{
    LauncherCatalogEntry, LauncherCatalogResponse, LauncherExecuteRequest, LauncherExecuteResponse,
    LauncherSchemaQuery, LauncherSchemaResponse, LauncherSearchQuery, LauncherSearchResponse,
};
pub use router::router;
pub use state::PaletteState;

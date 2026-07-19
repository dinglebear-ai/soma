//! Resource CRUD surfaces. Each submodule owns one Incus resource type and
//! is implemented independently against the stub files created here -
//! `instances` in one pass, `images`/`networks`/`storage`/`projects`
//! together in another, since they don't share any files.

pub mod images;
pub mod instances;
pub mod networks;
pub mod projects;
pub mod storage;

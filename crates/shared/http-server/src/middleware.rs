//! Generic request middleware: request IDs, tracing, timeouts, body limits,
//! and CORS. Every helper here takes its policy (durations, byte limits,
//! allowed origins/methods/headers) as parameters — none of it is Soma
//! policy, only Axum/tower-http mechanics.

pub mod body_limit;
pub mod cors;
pub mod request_id;
pub mod timeout;
pub mod tracing;

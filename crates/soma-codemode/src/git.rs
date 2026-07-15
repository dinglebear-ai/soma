pub mod command;
pub mod output;
pub mod provider;
pub mod provider_dispatch;
pub mod safety;

#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod provider_dispatch_tests;
#[cfg(test)]
mod provider_tests;
#[cfg(test)]
mod safety_tests;

pub use provider::GitProvider;

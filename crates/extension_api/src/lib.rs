//! Traits and error types the app uses to talk to extension providers.
#![forbid(unsafe_code)]

mod gateway_error;
mod provider_gateway;

pub use gateway_error::GatewayError;
pub use provider_gateway::ProviderGateway;

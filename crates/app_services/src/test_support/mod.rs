//! Fakes shared by the service tests.
mod fake_gateway;
mod fake_player;
mod fixed_clock;

pub(crate) use fake_gateway::FakeGateway;
pub(crate) use fake_player::FakePlayer;
pub(crate) use fixed_clock::FixedClock;

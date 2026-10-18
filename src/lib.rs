mod client;
pub mod errors;
pub mod types;

#[cfg(test)]
mod testing;

pub use client::{PlateAPI, VehicleWalker, DEFAULT_WALK_DELAY_MS, VERSION};
pub use errors::PlateAPIError;
pub use types::*;

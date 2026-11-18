//! Platform-independent logic: config, actions, key names, profile resolution.
//! Nothing here may depend on OS-specific types.

pub mod action;
pub mod button;
pub mod config;
pub mod keys;
pub mod profiles;
pub mod traits;

pub use action::{Action, Combo, DpiCmd, Preset};
pub use button::Button;
pub use config::Config;

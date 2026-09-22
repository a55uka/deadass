pub mod config;
pub mod toys;
pub mod updates;
pub mod views;

use deadass_companion::pipeline::Pipeline;
use std::sync::Arc;

pub type Backend = Arc<Pipeline>;

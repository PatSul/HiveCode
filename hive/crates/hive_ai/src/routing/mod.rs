//! Hive AI Routing System
//!
//! Intelligent model routing based on request complexity analysis,
//! provider availability tracking, automatic fallback chains, and
//! model capability scoring.

mod auto_fallback;
pub mod capability_router;
mod complexity_classifier;
mod model_router;

pub use auto_fallback::*;
pub use capability_router::*;
pub use complexity_classifier::*;
pub use model_router::*;

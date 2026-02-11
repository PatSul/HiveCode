//! Hive AI Routing System
//!
//! Intelligent model routing based on request complexity analysis,
//! provider availability tracking, and automatic fallback chains.

mod auto_fallback;
mod complexity_classifier;
mod model_router;

pub use auto_fallback::*;
pub use complexity_classifier::*;
pub use model_router::*;

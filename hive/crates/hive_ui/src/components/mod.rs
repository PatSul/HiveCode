pub mod code_block;
pub mod connectivity_badge;
pub mod diff_viewer;
pub mod message_bubble;
pub mod model_selector;
pub mod thinking_indicator;
pub mod toast;
pub mod wallet_card;
pub mod wizard_stepper;
pub mod context_attachment;
pub mod split_pane;

// Re-export key types for convenience.
pub use code_block::render_code_block;
pub use connectivity_badge::{render_connectivity_badge, ConnectivityState};
pub use context_attachment::{render_context_attachment, AttachedContext, AttachedFile};
pub use diff_viewer::{render_diff, DiffLine};
pub use message_bubble::{render_ai_message, render_error_message, render_user_message};
pub use model_selector::{render_model_badge, ModelSelected, ModelSelectorView};
pub use split_pane::{render_split_pane, PaneLayout, SplitDirection, TilingState};
pub use thinking_indicator::{render_thinking_indicator, ThinkingPhase};
pub use toast::{render_toast, ToastKind};
pub use wallet_card::render_wallet_card;
pub use wizard_stepper::render_wizard_stepper;

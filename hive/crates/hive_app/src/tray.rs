//! System tray service for the Hive desktop application.
//!
//! Provides a tray icon with a dynamic context menu showing:
//! - Show/Hide toggle for the main window
//! - Current AI model name
//! - Current session cost
//! - Quit action
//!
//! The tray icon is the Hive bee logo (decoded from embedded PNG, resized to 32×32).
//! Menu events are consumed via a channel and dispatched to a caller-supplied
//! callback on a background thread.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;

use anyhow::{Context, Result};
use tracing::{debug, error, info};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Width and height of the generated tray icon in pixels.
const ICON_SIZE: u32 = 32;

/// Hive accent colour — #00D4FF (cyan).
const ACCENT_R: u8 = 0x00;
const ACCENT_G: u8 = 0xD4;
const ACCENT_B: u8 = 0xFF;
const ACCENT_A: u8 = 0xFF;

// ---------------------------------------------------------------------------
// Menu item identifiers (kept as module-level constants for clarity)
// ---------------------------------------------------------------------------

const LABEL_SHOW: &str = "Show Hive";
const LABEL_HIDE: &str = "Hide Hive";
const LABEL_QUIT: &str = "Quit Hive";

// ---------------------------------------------------------------------------
// TrayEvent — what the event loop thread communicates back to the caller
// ---------------------------------------------------------------------------

/// Actions that the tray menu can emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// The user clicked "Show Hive" or "Hide Hive".
    ToggleVisibility,
    /// The user clicked "Quit Hive".
    Quit,
}

// ---------------------------------------------------------------------------
// TrayService
// ---------------------------------------------------------------------------

/// Manages the system-tray icon, its context menu, and the background thread
/// that listens for menu click events.
///
/// # Ownership
///
/// `TrayService` holds the `TrayIcon` (which **must** be kept alive for the
/// icon to remain visible) and `Arc`-shared handles to every `MenuItem` so
/// that the public update methods can mutate labels from any thread.
pub struct TrayService {
    /// The underlying tray icon handle.  Dropping this removes the icon.
    _tray_icon: TrayIcon,

    /// "Show Hive" / "Hide Hive" toggle item.
    toggle_item: MenuItem,

    /// Read-only display of the current model (disabled, non-clickable).
    model_item: MenuItem,

    /// Read-only display of the session cost (disabled, non-clickable).
    cost_item: MenuItem,

    /// Flag used to signal the event-polling thread to stop.
    running: Arc<AtomicBool>,

    /// Join handle for the background event thread.
    _event_thread: Option<thread::JoinHandle<()>>,
}

impl TrayService {
    // ---------------------------------------------------------------------
    // Construction
    // ---------------------------------------------------------------------

    /// Create the tray icon, build the context menu, and spawn the background
    /// event thread.
    ///
    /// `on_event` is called **on the background thread** whenever the user
    /// interacts with a menu item.  The caller is responsible for marshalling
    /// the event back to the main / UI thread if needed (e.g. via a channel
    /// or GPUI `cx.update_global`).
    ///
    /// Returns `Err` if the tray icon could not be created (e.g. on a headless
    /// system or inside a CI runner).
    pub fn new<F>(on_event: F) -> Result<Self>
    where
        F: Fn(TrayEvent) + Send + 'static,
    {
        // -- Icon -----------------------------------------------------------
        let icon = Self::create_icon()
            .or_else(|_| Self::create_accent_icon())
            .context("Failed to create tray icon bitmap")?;

        // -- Menu items -----------------------------------------------------
        let toggle_item = MenuItem::new(LABEL_SHOW, true, None);
        let model_item = MenuItem::new("Model: (none)", false, None);
        let cost_item = MenuItem::new("Cost: $0.0000", false, None);
        let quit_item = MenuItem::new(LABEL_QUIT, true, None);

        // -- Assemble menu --------------------------------------------------
        let menu = Menu::new();
        menu.append_items(&[
            &toggle_item,
            &model_item,
            &cost_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ])
        .context("Failed to build tray menu")?;

        // -- Build tray icon ------------------------------------------------
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Hive")
            .with_icon(icon)
            .with_menu_on_left_click(true)
            .build()
            .context("Failed to build TrayIcon — is a display server available?")?;

        info!("System tray icon created");

        // -- Background event thread ----------------------------------------
        let running = Arc::new(AtomicBool::new(true));

        let toggle_id = toggle_item.id().clone();
        let quit_id = quit_item.id().clone();
        let thread_running = Arc::clone(&running);

        let event_thread = thread::Builder::new()
            .name("hive-tray-events".into())
            .spawn(move || {
                Self::event_loop(
                    thread_running,
                    toggle_id,
                    quit_id,
                    on_event,
                );
            })
            .context("Failed to spawn tray event thread")?;

        Ok(Self {
            _tray_icon: tray_icon,
            toggle_item,
            model_item,
            cost_item,
            running,
            _event_thread: Some(event_thread),
        })
    }

    // ---------------------------------------------------------------------
    // Public update methods
    // ---------------------------------------------------------------------

    /// Update the model display label in the tray menu.
    ///
    /// Example: `tray.update_model("claude-sonnet-4-5")` sets the label to
    /// `"Model: claude-sonnet-4-5"`.
    pub fn update_model(&self, model: &str) {
        let label = format!("Model: {model}");
        self.model_item.set_text(&label);
        debug!("Tray model label updated to {label:?}");
    }

    /// Update the session cost display label in the tray menu.
    ///
    /// Example: `tray.update_cost(0.0042)` sets the label to
    /// `"Cost: $0.0042"`.
    pub fn update_cost(&self, cost: f64) {
        let label = format!("Cost: ${cost:.4}");
        self.cost_item.set_text(&label);
        debug!("Tray cost label updated to {label:?}");
    }

    /// Update the show/hide toggle label to reflect current window visibility.
    ///
    /// Pass `true` when the window **is** visible (so the label becomes
    /// "Hide Hive"), or `false` when the window is hidden (label becomes
    /// "Show Hive").
    pub fn set_visible(&self, visible: bool) {
        let label = if visible { LABEL_HIDE } else { LABEL_SHOW };
        self.toggle_item.set_text(label);
        debug!("Tray toggle label updated to {label:?}");
    }

    // ---------------------------------------------------------------------
    // Icon generation
    // ---------------------------------------------------------------------

    /// Load the Hive bee icon from the embedded PNG asset, resized to 32x32.
    /// Falls back to a solid accent-color square if decoding fails.
    fn create_icon() -> Result<Icon> {
        let png_bytes = include_bytes!("../../../assets/hive_bee.png");
        let img = image::load_from_memory(png_bytes)
            .context("Failed to decode embedded hive_bee.png")?;
        let resized = img.resize(
            ICON_SIZE,
            ICON_SIZE,
            image::imageops::FilterType::Nearest,
        );
        let rgba = resized.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        Icon::from_rgba(rgba.into_raw(), w, h)
            .context("Icon::from_rgba failed for bee icon")
    }

    /// Fallback: generate a solid-color 32x32 RGBA icon using the Hive accent colour.
    fn create_accent_icon() -> Result<Icon> {
        let pixel_count = (ICON_SIZE * ICON_SIZE) as usize;
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for _ in 0..pixel_count {
            rgba.push(ACCENT_R);
            rgba.push(ACCENT_G);
            rgba.push(ACCENT_B);
            rgba.push(ACCENT_A);
        }
        Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE)
            .context("Icon::from_rgba failed for accent icon")
    }

    // ---------------------------------------------------------------------
    // Event loop (runs on background thread)
    // ---------------------------------------------------------------------

    /// Poll `MenuEvent::receiver()` until `running` is set to `false`.
    ///
    /// Matching is done by comparing the event's menu-item ID against the
    /// known IDs captured at construction time.  Unknown IDs are silently
    /// ignored (they correspond to the disabled info items).
    fn event_loop<F>(
        running: Arc<AtomicBool>,
        toggle_id: tray_icon::menu::MenuId,
        quit_id: tray_icon::menu::MenuId,
        on_event: F,
    ) where
        F: Fn(TrayEvent) + Send + 'static,
    {
        let receiver = MenuEvent::receiver();

        while running.load(Ordering::Relaxed) {
            // Use a short timeout so we can check the `running` flag
            // periodically without burning CPU.
            match receiver.try_recv() {
                Ok(event) => {
                    if event.id == toggle_id {
                        debug!("Tray: toggle visibility clicked");
                        on_event(TrayEvent::ToggleVisibility);
                    } else if event.id == quit_id {
                        debug!("Tray: quit clicked");
                        on_event(TrayEvent::Quit);
                        // The callback will handle actual shutdown; we just
                        // stop our own loop.
                        running.store(false, Ordering::Relaxed);
                    }
                    // Other IDs are the disabled info items — nothing to do.
                }
                Err(_) => {
                    // No event available — sleep briefly to avoid busy-spin.
                    thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }

        info!("Tray event loop exiting");
    }
}

impl Drop for TrayService {
    fn drop(&mut self) {
        // Signal the background thread to exit.
        self.running.store(false, Ordering::Relaxed);
        // We intentionally do *not* join the thread here to avoid blocking
        // the main thread during shutdown.  The thread will notice the flag
        // within ~50 ms and exit on its own.
        info!("TrayService dropped — tray icon removed");
    }
}

// ---------------------------------------------------------------------------
// Convenience constructor for headless-safe initialisation
// ---------------------------------------------------------------------------

/// Try to create a `TrayService`.  If the tray cannot be created (headless
/// system, Wayland without StatusNotifierItem support, CI, etc.) this logs a
/// warning and returns `None` instead of propagating the error.
pub fn try_create_tray<F>(on_event: F) -> Option<TrayService>
where
    F: Fn(TrayEvent) + Send + 'static,
{
    match TrayService::new(on_event) {
        Ok(service) => Some(service),
        Err(e) => {
            error!("System tray unavailable: {e:#}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bee_icon_loads_successfully() {
        let icon = TrayService::create_icon();
        assert!(icon.is_ok(), "bee icon creation should succeed");
    }

    #[test]
    fn accent_fallback_icon_works() {
        let icon = TrayService::create_accent_icon();
        assert!(icon.is_ok(), "accent icon creation should succeed");
    }

    #[test]
    fn tray_event_variants_are_comparable() {
        assert_eq!(TrayEvent::ToggleVisibility, TrayEvent::ToggleVisibility);
        assert_eq!(TrayEvent::Quit, TrayEvent::Quit);
        assert_ne!(TrayEvent::ToggleVisibility, TrayEvent::Quit);
    }

    #[test]
    fn label_constants_are_non_empty() {
        assert!(!LABEL_SHOW.is_empty());
        assert!(!LABEL_HIDE.is_empty());
        assert!(!LABEL_QUIT.is_empty());
    }
}

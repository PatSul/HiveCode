use enigo::{Enigo, Settings, Mouse, Keyboard, Button, Key, Coordinate};
use anyhow::Result;

/// Provides UI automation capabilities (mouse/keyboard) via enigo.
pub struct UiDriver {
    enigo: Enigo,
}

impl UiDriver {
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("Failed to initialize enigo: {}", e))?;
        Ok(Self { enigo })
    }

    /// Simulates a mouse click at the given coordinates.
    pub fn click(&mut self, x: i32, y: i32) -> Result<()> {
        self.enigo.move_mouse(x, y, Coordinate::Abs).map_err(|e| anyhow::anyhow!("Move mouse failed: {}", e))?;
        self.enigo.button(Button::Left, enigo::Direction::Click).map_err(|e| anyhow::anyhow!("Mouse click failed: {}", e))?;
        Ok(())
    }

    /// Simulates typing text at the current cursor location.
    pub fn type_text(&mut self, text: &str) -> Result<()> {
        self.enigo.text(text).map_err(|e| anyhow::anyhow!("Type text failed: {}", e))?;
        Ok(())
    }
    
    /// Simulates pressing a specific key, like Enter.
    pub fn press_enter(&mut self) -> Result<()> {
        self.enigo.key(Key::Return, enigo::Direction::Click).map_err(|e| anyhow::anyhow!("Press enter failed: {}", e))?;
        Ok(())
    }
}

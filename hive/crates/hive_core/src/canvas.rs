use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The kind of element that can be placed on a canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ElementType {
    Rectangle,
    Circle,
    Text,
    Line,
    Arrow,
    Image,
    Sticky,
    Group,
}

impl ElementType {
    /// Human-readable label for the element type.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Circle => "Circle",
            Self::Text => "Text",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::Image => "Image",
            Self::Sticky => "Sticky",
            Self::Group => "Group",
        }
    }

    /// All variants in definition order.
    pub fn all() -> [Self; 8] {
        [
            Self::Rectangle,
            Self::Circle,
            Self::Text,
            Self::Line,
            Self::Arrow,
            Self::Image,
            Self::Sticky,
            Self::Group,
        ]
    }
}

// ---------------------------------------------------------------------------
// Geometry primitives
// ---------------------------------------------------------------------------

/// A 2-D point on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Width/height dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single element on the canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasElement {
    pub id: String,
    pub element_type: ElementType,
    pub position: Point,
    pub size: Size,
    pub content: Option<String>,
    pub color: Option<String>,
    pub z_index: i32,
    pub locked: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A directed connection between two elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub from_element_id: String,
    pub to_element_id: String,
    pub label: Option<String>,
}

/// Serializable snapshot of the full canvas state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasState {
    pub id: String,
    pub name: String,
    pub elements: Vec<CanvasElement>,
    pub connections: Vec<Connection>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// LiveCanvas — in-memory interactive canvas
// ---------------------------------------------------------------------------

/// In-memory live canvas with element and connection management.
pub struct LiveCanvas {
    id: String,
    name: String,
    elements: Vec<CanvasElement>,
    connections: Vec<Connection>,
    next_z_index: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl LiveCanvas {
    /// Creates a new, empty canvas with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        let canvas = Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            elements: Vec::new(),
            connections: Vec::new(),
            next_z_index: 1,
            created_at: now,
            updated_at: now,
        };
        debug!("Created new canvas: {} ({})", canvas.name, canvas.id);
        canvas
    }

    // -----------------------------------------------------------------------
    // Element CRUD
    // -----------------------------------------------------------------------

    /// Adds a new element to the canvas and returns its ID.
    pub fn add_element(
        &mut self,
        element_type: ElementType,
        position: Point,
        size: Size,
    ) -> String {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let z_index = self.next_z_index;
        self.next_z_index += 1;

        let element = CanvasElement {
            id: id.clone(),
            element_type,
            position,
            size,
            content: None,
            color: None,
            z_index,
            locked: false,
            created_at: now,
            updated_at: now,
        };

        debug!(
            "Added {} element {} at ({}, {})",
            element_type.label(),
            id,
            position.x,
            position.y
        );

        self.elements.push(element);
        self.updated_at = now;
        id
    }

    /// Partially updates an element. Pass `None` for fields that should remain
    /// unchanged.
    pub fn update_element(
        &mut self,
        id: &str,
        position: Option<Point>,
        size: Option<Size>,
        content: Option<Option<String>>,
        color: Option<Option<String>>,
    ) -> Result<()> {
        let element = self
            .elements
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("Element not found: {}", id))?;

        if element.locked {
            bail!("Element is locked: {}", id);
        }

        if let Some(pos) = position {
            element.position = pos;
        }
        if let Some(s) = size {
            element.size = s;
        }
        if let Some(c) = content {
            element.content = c;
        }
        if let Some(col) = color {
            element.color = col;
        }
        element.updated_at = Utc::now();
        self.updated_at = element.updated_at;
        Ok(())
    }

    /// Removes an element by ID, along with any connections that reference it.
    pub fn remove_element(&mut self, id: &str) -> Result<()> {
        let pos = self
            .elements
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("Element not found: {}", id))?;

        self.elements.remove(pos);

        // Remove all connections that reference this element.
        self.connections
            .retain(|c| c.from_element_id != id && c.to_element_id != id);

        self.updated_at = Utc::now();
        debug!("Removed element {} and its connections", id);
        Ok(())
    }

    /// Moves an element to a new position.
    pub fn move_element(&mut self, id: &str, new_position: Point) -> Result<()> {
        let element = self
            .elements
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("Element not found: {}", id))?;

        if element.locked {
            bail!("Element is locked: {}", id);
        }

        element.position = new_position;
        element.updated_at = Utc::now();
        self.updated_at = element.updated_at;
        Ok(())
    }

    /// Resizes an element.
    pub fn resize_element(&mut self, id: &str, new_size: Size) -> Result<()> {
        let element = self
            .elements
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("Element not found: {}", id))?;

        if element.locked {
            bail!("Element is locked: {}", id);
        }

        element.size = new_size;
        element.updated_at = Utc::now();
        self.updated_at = element.updated_at;
        Ok(())
    }

    /// Locks an element, preventing moves, resizes, and updates.
    pub fn lock_element(&mut self, id: &str) -> Result<()> {
        let element = self
            .elements
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("Element not found: {}", id))?;

        element.locked = true;
        element.updated_at = Utc::now();
        self.updated_at = element.updated_at;
        Ok(())
    }

    /// Unlocks an element, allowing moves, resizes, and updates again.
    pub fn unlock_element(&mut self, id: &str) -> Result<()> {
        let element = self
            .elements
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("Element not found: {}", id))?;

        element.locked = false;
        element.updated_at = Utc::now();
        self.updated_at = element.updated_at;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Connections
    // -----------------------------------------------------------------------

    /// Connects two elements with an optional label. Returns the connection ID.
    /// Both elements must exist and must be different.
    pub fn add_connection(
        &mut self,
        from_id: &str,
        to_id: &str,
        label: Option<String>,
    ) -> Result<String> {
        if from_id == to_id {
            bail!("Cannot connect an element to itself");
        }

        if !self.elements.iter().any(|e| e.id == from_id) {
            bail!("Source element not found: {}", from_id);
        }
        if !self.elements.iter().any(|e| e.id == to_id) {
            bail!("Target element not found: {}", to_id);
        }

        let id = Uuid::new_v4().to_string();
        let connection = Connection {
            id: id.clone(),
            from_element_id: from_id.to_string(),
            to_element_id: to_id.to_string(),
            label,
        };

        debug!("Added connection {} -> {}", from_id, to_id);
        self.connections.push(connection);
        self.updated_at = Utc::now();
        Ok(id)
    }

    /// Removes a connection by ID.
    pub fn remove_connection(&mut self, id: &str) -> Result<()> {
        let pos = self
            .connections
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| anyhow::anyhow!("Connection not found: {}", id))?;

        self.connections.remove(pos);
        self.updated_at = Utc::now();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Returns a reference to an element by ID.
    pub fn get_element(&self, id: &str) -> Option<&CanvasElement> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// Returns a slice of all elements.
    pub fn list_elements(&self) -> &[CanvasElement] {
        &self.elements
    }

    /// Returns a slice of all connections.
    pub fn get_connections(&self) -> &[Connection] {
        &self.connections
    }

    /// Hit-test: returns references to all elements whose bounding box contains
    /// the given point. Elements are returned in z-index order (highest first)
    /// so the topmost element is first.
    pub fn elements_at_point(&self, point: Point) -> Vec<&CanvasElement> {
        let mut hits: Vec<&CanvasElement> = self
            .elements
            .iter()
            .filter(|e| {
                point.x >= e.position.x
                    && point.x <= e.position.x + e.size.width
                    && point.y >= e.position.y
                    && point.y <= e.position.y + e.size.height
            })
            .collect();

        // Sort by z_index descending so the topmost element is first.
        hits.sort_by(|a, b| b.z_index.cmp(&a.z_index));
        hits
    }

    /// Returns the number of elements on the canvas.
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Returns the number of connections on the canvas.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    // -----------------------------------------------------------------------
    // Serialization
    // -----------------------------------------------------------------------

    /// Serializes the full canvas state to a JSON string.
    pub fn to_json(&self) -> Result<String> {
        let state = CanvasState {
            id: self.id.clone(),
            name: self.name.clone(),
            elements: self.elements.clone(),
            connections: self.connections.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        };
        serde_json::to_string_pretty(&state).context("Failed to serialize canvas state")
    }

    /// Deserializes a `LiveCanvas` from a JSON string previously produced by
    /// [`to_json`](Self::to_json).
    pub fn from_json(json: &str) -> Result<Self> {
        let state: CanvasState =
            serde_json::from_str(json).context("Failed to deserialize canvas state")?;

        let max_z = state
            .elements
            .iter()
            .map(|e| e.z_index)
            .max()
            .unwrap_or(0);

        Ok(Self {
            id: state.id,
            name: state.name,
            elements: state.elements,
            connections: state.connections,
            next_z_index: max_z + 1,
            created_at: state.created_at,
            updated_at: state.updated_at,
        })
    }
}

impl Default for LiveCanvas {
    fn default() -> Self {
        Self::new("Untitled Canvas")
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper
    // -----------------------------------------------------------------------

    fn make_canvas() -> LiveCanvas {
        LiveCanvas::new("Test Canvas")
    }

    fn default_point() -> Point {
        Point::new(100.0, 200.0)
    }

    fn default_size() -> Size {
        Size::new(50.0, 30.0)
    }

    // -----------------------------------------------------------------------
    // 1. new canvas
    // -----------------------------------------------------------------------

    #[test]
    fn new_canvas_is_empty() {
        let canvas = make_canvas();
        assert_eq!(canvas.element_count(), 0);
        assert_eq!(canvas.connection_count(), 0);
        assert!(canvas.list_elements().is_empty());
        assert!(canvas.get_connections().is_empty());
        assert!(!canvas.id.is_empty());
    }

    // -----------------------------------------------------------------------
    // 2. add_element
    // -----------------------------------------------------------------------

    #[test]
    fn add_element_returns_id_and_increments_count() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Rectangle, default_point(), default_size());

        assert!(!id.is_empty());
        assert_eq!(canvas.element_count(), 1);

        let elem = canvas.get_element(&id).unwrap();
        assert_eq!(elem.element_type, ElementType::Rectangle);
        assert_eq!(elem.position, default_point());
        assert_eq!(elem.size, default_size());
        assert!(!elem.locked);
        assert!(elem.content.is_none());
        assert!(elem.color.is_none());
    }

    // -----------------------------------------------------------------------
    // 3. z_index auto-increment
    // -----------------------------------------------------------------------

    #[test]
    fn z_index_auto_increments() {
        let mut canvas = make_canvas();
        let id1 = canvas.add_element(ElementType::Rectangle, default_point(), default_size());
        let id2 = canvas.add_element(ElementType::Circle, default_point(), default_size());
        let id3 = canvas.add_element(ElementType::Text, default_point(), default_size());

        let z1 = canvas.get_element(&id1).unwrap().z_index;
        let z2 = canvas.get_element(&id2).unwrap().z_index;
        let z3 = canvas.get_element(&id3).unwrap().z_index;

        assert!(z1 < z2);
        assert!(z2 < z3);
    }

    // -----------------------------------------------------------------------
    // 4. update_element (partial)
    // -----------------------------------------------------------------------

    #[test]
    fn update_element_partial_fields() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Sticky, default_point(), default_size());

        canvas
            .update_element(
                &id,
                Some(Point::new(10.0, 20.0)),
                None,
                Some(Some("Hello".into())),
                Some(Some("#ff0000".into())),
            )
            .unwrap();

        let elem = canvas.get_element(&id).unwrap();
        assert_eq!(elem.position, Point::new(10.0, 20.0));
        assert_eq!(elem.size, default_size()); // unchanged
        assert_eq!(elem.content.as_deref(), Some("Hello"));
        assert_eq!(elem.color.as_deref(), Some("#ff0000"));
    }

    // -----------------------------------------------------------------------
    // 5. update locked element fails
    // -----------------------------------------------------------------------

    #[test]
    fn update_locked_element_fails() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Text, default_point(), default_size());

        canvas.lock_element(&id).unwrap();

        let result = canvas.update_element(&id, Some(Point::new(0.0, 0.0)), None, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("locked"));
    }

    // -----------------------------------------------------------------------
    // 6. remove_element removes connections too
    // -----------------------------------------------------------------------

    #[test]
    fn remove_element_cascades_connections() {
        let mut canvas = make_canvas();
        let id1 = canvas.add_element(ElementType::Rectangle, Point::new(0.0, 0.0), default_size());
        let id2 =
            canvas.add_element(ElementType::Circle, Point::new(100.0, 0.0), default_size());
        let id3 =
            canvas.add_element(ElementType::Text, Point::new(200.0, 0.0), default_size());

        canvas.add_connection(&id1, &id2, None).unwrap();
        canvas
            .add_connection(&id2, &id3, Some("link".into()))
            .unwrap();
        assert_eq!(canvas.connection_count(), 2);

        // Removing id2 should remove both connections.
        canvas.remove_element(&id2).unwrap();
        assert_eq!(canvas.element_count(), 2);
        assert_eq!(canvas.connection_count(), 0);
    }

    // -----------------------------------------------------------------------
    // 7. remove nonexistent element
    // -----------------------------------------------------------------------

    #[test]
    fn remove_nonexistent_element_fails() {
        let mut canvas = make_canvas();
        let result = canvas.remove_element("ghost");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 8. move_element
    // -----------------------------------------------------------------------

    #[test]
    fn move_element_updates_position() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Arrow, default_point(), default_size());

        let new_pos = Point::new(500.0, 600.0);
        canvas.move_element(&id, new_pos).unwrap();

        assert_eq!(canvas.get_element(&id).unwrap().position, new_pos);
    }

    #[test]
    fn move_locked_element_fails() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Line, default_point(), default_size());

        canvas.lock_element(&id).unwrap();
        let result = canvas.move_element(&id, Point::new(0.0, 0.0));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("locked"));
    }

    // -----------------------------------------------------------------------
    // 9. resize_element
    // -----------------------------------------------------------------------

    #[test]
    fn resize_element_updates_size() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Image, default_point(), default_size());

        let new_size = Size::new(200.0, 150.0);
        canvas.resize_element(&id, new_size).unwrap();

        assert_eq!(canvas.get_element(&id).unwrap().size, new_size);
    }

    #[test]
    fn resize_locked_element_fails() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Rectangle, default_point(), default_size());

        canvas.lock_element(&id).unwrap();
        let result = canvas.resize_element(&id, Size::new(10.0, 10.0));
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 10. lock / unlock
    // -----------------------------------------------------------------------

    #[test]
    fn lock_and_unlock_element() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Group, default_point(), default_size());

        assert!(!canvas.get_element(&id).unwrap().locked);

        canvas.lock_element(&id).unwrap();
        assert!(canvas.get_element(&id).unwrap().locked);

        canvas.unlock_element(&id).unwrap();
        assert!(!canvas.get_element(&id).unwrap().locked);

        // After unlock, mutations should work again.
        canvas
            .move_element(&id, Point::new(999.0, 999.0))
            .unwrap();
        assert_eq!(
            canvas.get_element(&id).unwrap().position,
            Point::new(999.0, 999.0)
        );
    }

    // -----------------------------------------------------------------------
    // 11. add_connection
    // -----------------------------------------------------------------------

    #[test]
    fn add_connection_between_elements() {
        let mut canvas = make_canvas();
        let id1 = canvas.add_element(ElementType::Rectangle, default_point(), default_size());
        let id2 =
            canvas.add_element(ElementType::Circle, Point::new(300.0, 200.0), default_size());

        let conn_id = canvas
            .add_connection(&id1, &id2, Some("relates to".into()))
            .unwrap();
        assert!(!conn_id.is_empty());
        assert_eq!(canvas.connection_count(), 1);

        let conn = &canvas.get_connections()[0];
        assert_eq!(conn.from_element_id, id1);
        assert_eq!(conn.to_element_id, id2);
        assert_eq!(conn.label.as_deref(), Some("relates to"));
    }

    // -----------------------------------------------------------------------
    // 12. add_connection self-reference blocked
    // -----------------------------------------------------------------------

    #[test]
    fn add_connection_self_reference_blocked() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Sticky, default_point(), default_size());

        let result = canvas.add_connection(&id, &id, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("itself"));
    }

    // -----------------------------------------------------------------------
    // 13. add_connection with missing element
    // -----------------------------------------------------------------------

    #[test]
    fn add_connection_missing_element_fails() {
        let mut canvas = make_canvas();
        let id = canvas.add_element(ElementType::Text, default_point(), default_size());

        let result = canvas.add_connection(&id, "nonexistent", None);
        assert!(result.is_err());

        let result = canvas.add_connection("nonexistent", &id, None);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 14. remove_connection
    // -----------------------------------------------------------------------

    #[test]
    fn remove_connection_by_id() {
        let mut canvas = make_canvas();
        let id1 = canvas.add_element(ElementType::Rectangle, default_point(), default_size());
        let id2 =
            canvas.add_element(ElementType::Circle, Point::new(300.0, 0.0), default_size());

        let conn_id = canvas.add_connection(&id1, &id2, None).unwrap();
        assert_eq!(canvas.connection_count(), 1);

        canvas.remove_connection(&conn_id).unwrap();
        assert_eq!(canvas.connection_count(), 0);
    }

    #[test]
    fn remove_nonexistent_connection_fails() {
        let mut canvas = make_canvas();
        let result = canvas.remove_connection("ghost");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // 15. elements_at_point (hit test)
    // -----------------------------------------------------------------------

    #[test]
    fn elements_at_point_hit_test() {
        let mut canvas = make_canvas();

        // Element at (10, 10) with size (100, 100) -> covers (10..110, 10..110)
        let id1 =
            canvas.add_element(ElementType::Rectangle, Point::new(10.0, 10.0), Size::new(100.0, 100.0));

        // Overlapping element at (50, 50) with size (100, 100) -> covers (50..150, 50..150)
        let id2 =
            canvas.add_element(ElementType::Circle, Point::new(50.0, 50.0), Size::new(100.0, 100.0));

        // Non-overlapping element at (500, 500)
        let _id3 =
            canvas.add_element(ElementType::Text, Point::new(500.0, 500.0), Size::new(20.0, 20.0));

        // Point in overlap region of id1 and id2.
        let hits = canvas.elements_at_point(Point::new(75.0, 75.0));
        assert_eq!(hits.len(), 2);
        // id2 has higher z_index, should be first.
        assert_eq!(hits[0].id, id2);
        assert_eq!(hits[1].id, id1);

        // Point only in id1.
        let hits = canvas.elements_at_point(Point::new(15.0, 15.0));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, id1);

        // Point outside all elements.
        let hits = canvas.elements_at_point(Point::new(999.0, 999.0));
        assert!(hits.is_empty());
    }

    // -----------------------------------------------------------------------
    // 16. element_count / connection_count
    // -----------------------------------------------------------------------

    #[test]
    fn counts_track_additions_and_removals() {
        let mut canvas = make_canvas();
        assert_eq!(canvas.element_count(), 0);
        assert_eq!(canvas.connection_count(), 0);

        let id1 = canvas.add_element(ElementType::Rectangle, default_point(), default_size());
        let id2 = canvas.add_element(ElementType::Circle, default_point(), default_size());
        assert_eq!(canvas.element_count(), 2);

        canvas.add_connection(&id1, &id2, None).unwrap();
        assert_eq!(canvas.connection_count(), 1);

        canvas.remove_element(&id1).unwrap();
        assert_eq!(canvas.element_count(), 1);
        assert_eq!(canvas.connection_count(), 0); // cascaded
    }

    // -----------------------------------------------------------------------
    // 17. to_json / from_json round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn json_round_trip() {
        let mut canvas = make_canvas();
        let id1 = canvas.add_element(ElementType::Sticky, Point::new(10.0, 20.0), Size::new(80.0, 60.0));
        canvas
            .update_element(
                &id1,
                None,
                None,
                Some(Some("Note".into())),
                Some(Some("#ffcc00".into())),
            )
            .unwrap();

        let id2 = canvas.add_element(ElementType::Arrow, Point::new(200.0, 300.0), Size::new(5.0, 100.0));
        canvas.add_connection(&id1, &id2, Some("points to".into())).unwrap();
        canvas.lock_element(&id1).unwrap();

        let json = canvas.to_json().unwrap();
        let restored = LiveCanvas::from_json(&json).unwrap();

        assert_eq!(restored.element_count(), 2);
        assert_eq!(restored.connection_count(), 1);

        let elem1 = restored.get_element(&id1).unwrap();
        assert_eq!(elem1.element_type, ElementType::Sticky);
        assert_eq!(elem1.position, Point::new(10.0, 20.0));
        assert_eq!(elem1.content.as_deref(), Some("Note"));
        assert_eq!(elem1.color.as_deref(), Some("#ffcc00"));
        assert!(elem1.locked);

        let conn = &restored.get_connections()[0];
        assert_eq!(conn.from_element_id, id1);
        assert_eq!(conn.to_element_id, id2);
        assert_eq!(conn.label.as_deref(), Some("points to"));
    }

    // -----------------------------------------------------------------------
    // 18. from_json restores z_index correctly
    // -----------------------------------------------------------------------

    #[test]
    fn from_json_restores_next_z_index() {
        let mut canvas = make_canvas();
        canvas.add_element(ElementType::Rectangle, default_point(), default_size());
        canvas.add_element(ElementType::Circle, default_point(), default_size());
        canvas.add_element(ElementType::Text, default_point(), default_size());

        let json = canvas.to_json().unwrap();
        let mut restored = LiveCanvas::from_json(&json).unwrap();

        // Adding a new element after restoration should get a z_index higher
        // than all existing elements.
        let new_id = restored.add_element(ElementType::Line, default_point(), default_size());
        let max_existing = restored
            .list_elements()
            .iter()
            .filter(|e| e.id != new_id)
            .map(|e| e.z_index)
            .max()
            .unwrap();
        let new_z = restored.get_element(&new_id).unwrap().z_index;
        assert!(new_z > max_existing);
    }

    // -----------------------------------------------------------------------
    // 19. enum serde
    // -----------------------------------------------------------------------

    #[test]
    fn element_type_serde_round_trip() {
        for et in ElementType::all() {
            let json = serde_json::to_string(&et).unwrap();
            let parsed: ElementType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, et);
        }
    }

    // -----------------------------------------------------------------------
    // 20. default trait
    // -----------------------------------------------------------------------

    #[test]
    fn default_canvas_is_untitled() {
        let canvas = LiveCanvas::default();
        assert_eq!(canvas.name, "Untitled Canvas");
        assert!(canvas.list_elements().is_empty());
        assert!(canvas.get_connections().is_empty());
    }

    // -----------------------------------------------------------------------
    // 21. get_element not found
    // -----------------------------------------------------------------------

    #[test]
    fn get_nonexistent_element_returns_none() {
        let canvas = make_canvas();
        assert!(canvas.get_element("does-not-exist").is_none());
    }

    // -----------------------------------------------------------------------
    // 22. element_type labels
    // -----------------------------------------------------------------------

    #[test]
    fn element_type_labels_are_nonempty() {
        for et in ElementType::all() {
            assert!(!et.label().is_empty());
        }
    }
}

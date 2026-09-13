pub mod buffer;
pub mod cursor;
pub mod editor;
pub mod search;

pub use buffer::{Buffer, Position};
pub use cursor::{Cursor, Selection};
pub use editor::Editor;
pub use search::{find_all, SearchMatch};

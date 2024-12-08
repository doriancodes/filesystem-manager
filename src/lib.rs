// Public modules
pub mod modules;

// Re-export key types for easy access
pub use modules::filesystem::{BoundEntry, HelloFS};
pub use modules::mount::FilesystemManager;
pub use modules::namespace::{BindMode, NamespaceManager};

//! Platform-independent graph documents. All persistent edits go through commands.
mod command;
mod model;
mod spatial;
pub use command::*;
pub use model::*;
pub use spatial::*;

mod workspace;
pub use workspace::*;

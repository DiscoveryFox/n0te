//! n0te-core: the notebook model shared by the tablet, laptop and phone apps.
//!
//! A notebook is an *index* document (the tree of groups, sheets and side sheets) plus one
//! document per sheet (its objects: strokes, later text, images, …). Documents are Loro CRDTs,
//! so every device keeps a full copy and concurrent edits merge without conflicts.

pub mod arrows;
pub mod design;
pub mod error;
pub mod format;
pub mod ids;
pub mod ink;
pub mod inklines;
pub mod layout;
pub mod notebook;
pub mod pens;
pub mod plot;
pub mod search;
pub mod shapes;
pub mod sheet;
pub mod snap;
pub mod store;

pub use error::{Error, Result};

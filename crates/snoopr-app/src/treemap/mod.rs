//! Nested-folders treemap: the cached layout (`model`), its transitions
//! (`anim`) and its drawing (`paint`).

pub mod anim;
pub mod model;
pub mod paint;

pub use model::CachedLayout;
pub use paint::show;

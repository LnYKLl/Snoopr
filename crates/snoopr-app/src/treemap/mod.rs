//! Nested-folders treemap: the cached layout (`model`) and its drawing (`paint`).

pub mod model;
pub mod paint;

pub use model::CachedLayout;
pub use paint::show;

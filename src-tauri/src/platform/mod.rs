//! Operating-system integration kept outside the application/domain layers.
//!
//! The HTTP transport owns orchestration. These modules expose narrowly scoped
//! primitives and never accept arbitrary paths, shell commands or network URLs
//! from a browser request.

mod durability;
mod error;

pub mod discovery;
pub mod host_identity;
pub mod instance_lock;
pub mod pairing;
pub mod storage_volumes;
pub mod tls;
pub mod windows_service;

pub(crate) use durability::sync_directory;
pub use error::{PlatformError, PlatformResult};

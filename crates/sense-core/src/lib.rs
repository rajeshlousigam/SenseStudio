//! SenseStudio media engine: probe, check against the StepOut Ready spec,
//! plan the cheapest fix, render edits, and verify every result.
//! No UI dependencies; the only network access is the opt-in yt-dlp downloader.

pub mod atoms;
pub mod caps;
pub mod download;
pub mod error;
pub mod exec;
pub mod pipeline;
pub mod plan;
pub mod probe;
pub mod render;
pub mod spec;
pub mod tools;

pub use error::{Error, Result};

//! **arvidref** – reference management for arvidleaf.
//!
//! The library is a plain BibLaTeX file. This crate has no GUI dependency so it can
//! later back a standalone `arvidref` program as well.
//!
//! - [`bib`]: lossless BibLaTeX parser and writer (unchanged entries are written back
//!   byte for byte; broken entries are kept as raw text instead of failing the file)
//! - [`latex`]: LaTeX ↔ Unicode for display, search and writing
//! - [`names`]: BibTeX name lists (`von Last, Jr, First`)

pub mod bib;
pub mod latex;
pub mod names;

pub use bib::{Bib, Block, Entry, Field, Kind, Piece, Value};

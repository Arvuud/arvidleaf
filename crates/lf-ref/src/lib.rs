//! **arvidref** – reference management for arvidleaf.
//!
//! The library is a plain BibLaTeX file. This crate has no GUI dependency so it can
//! later back a standalone `arvidref` program as well.
//!
//! - [`bib`]: lossless BibLaTeX parser and writer (unchanged entries are written back
//!   byte for byte; broken entries are kept as raw text instead of failing the file)
//! - [`latex`]: LaTeX ↔ Unicode for display, search and writing
//! - [`names`]: BibTeX name lists (`von Last, Jr, First`)
//! - [`library`]: the central library (`library.bib`), search, duplicates, tags
//! - [`collections`]: named, nested groups of entries
//! - [`key`]: citation keys from a pattern
//! - [`import`]: BibTeX, CSL-JSON, PDFs; DOI/arXiv/ISBN lookups ([`net`], via curl)

pub mod bib;
pub mod collections;
pub mod import;
pub mod key;
pub mod latex;
pub mod library;
pub mod names;
pub mod net;

pub use bib::{Bib, Block, Entry, Field, Kind, Piece, Value};
pub use library::{EntryView, Library};

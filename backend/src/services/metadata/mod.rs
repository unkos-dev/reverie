//! Metadata extraction, validation, and storage for ingested `EPUB` files.
//!
//! Orchestrates the pipeline from raw `OPF` fields through sanitisation,
//! `ISBN` normalisation, inversion detection, and journal-row persistence.
//! All sub-modules are deterministic and do not perform I/O except `draft`,
//! which writes to an open `sqlx` connection supplied by the caller.

/// Write extracted metadata fields as `metadata_versions` journal rows.
pub mod draft;
/// Typed per-(scheme, level) parsing of external identifier values.
pub mod external_id;
/// Transform raw `OPF` data into [`crate::services::metadata::extractor::ExtractedMetadata`].
pub mod extractor;
/// Heuristic title-author inversion detection.
pub mod inversion;
/// `ISBN-10` / `ISBN-13` checksum validation, conversion, and normalisation.
pub mod isbn;
/// `HTML`-to-plaintext sanitisation pipeline for untrusted metadata text.
pub mod sanitiser;

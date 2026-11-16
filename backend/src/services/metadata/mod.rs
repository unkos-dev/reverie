//! Metadata extraction, validation, and storage for ingested `EPUB` files.
//!
//! Orchestrates the pipeline from raw `OPF` fields through sanitisation,
//! `ISBN` normalisation, inversion detection, and journal-row persistence.
//! Extraction and validation are deterministic. `draft` writes to a caller-supplied
//! SQLx connection; `editing` owns the write transactions shared by HTTP and GraphQL.

/// Write extracted metadata fields as `metadata_versions` journal rows.
pub mod draft;
/// Typed per-(scheme, level) parsing of external identifier values.
pub mod external_id;
/// Transform raw `OPF` data into [`crate::services::metadata::extractor::ExtractedMetadata`].
pub mod extractor;
/// Heuristic title-author inversion detection.
pub mod inversion;
/// `HTML`-to-plaintext sanitisation pipeline for untrusted metadata text.
pub mod sanitiser;

/// Precision-preserving validation of bibliographic publication dates.
pub mod publication_date;

pub(crate) mod editing;

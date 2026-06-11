//! Dry-run preview for the enrichment pipeline.
//!
//! Reuses the source fan-out and cache write steps from
//! `orchestrator` but does NOT touch `metadata_versions` or
//! canonical columns.  The caller receives an in-memory diff.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;

use super::orchestrator::fan_out_for_dry_run;
use super::policy::{self, Decision, PolicyInputRow};
use super::sources::SourceResult;
use super::value_hash;

/// The result of a dry-run enrichment pass for a single manifestation.
///
/// Contains three mutually exclusive lists: changes that would be applied
/// immediately, changes that would be staged for review, and fields that
/// are locked (silently skipped).  Source failures are surfaced separately
/// so callers can distinguish data results from infrastructure issues.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DryRunDiff {
    /// Manifestation that was evaluated.
    pub manifestation_id: Uuid,
    /// Parent work of the manifestation.
    pub work_id: Uuid,
    /// Fields that would be promoted to canonical (one entry per field).
    pub would_apply: Vec<FieldChange>,
    /// Fields that would be left as pending and require human review.
    pub would_stage: Vec<FieldChange>,
    /// Field names that were skipped because a user lock is active.
    pub locked: Vec<String>,
    /// Sources that returned an error during this run.
    pub source_failures: Vec<SourceFailureSummary>,
}

/// One proposed change to a single field from a single source.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FieldChange {
    /// Canonical field name (e.g. `"title"`, `"isbn_13"`).
    pub field_name: String,
    /// Source that produced this value (e.g. `"openlibrary"`, `"googlebooks"`).
    pub source_id: String,
    /// The proposed new value in its raw `JSON` form.
    pub new_value: serde_json::Value,
    /// Number of sources that reported the same value (quorum count).
    pub quorum: u32,
}

/// A brief summary of a source-level failure during a dry-run pass.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SourceFailureSummary {
    /// Source that failed (e.g. `"hardcover"`).
    pub source_id: String,
    /// Human-readable error description.
    pub error: String,
}

/// Simulate an enrichment run for `manifestation_id` and return an in-memory diff.
///
/// Reuses the full source fan-out path from `orchestrator` — including
/// cache writes — but does **not** mutate `metadata_versions`, canonical columns,
/// or `writeback_jobs`.  Safe to call on any manifestation without side-effects
/// beyond the `api_cache` table.
///
/// # Errors
///
/// Returns an error if the database is unreachable, the manifestation does not
/// exist, or the fan-out call fails at the infrastructure level.  Individual
/// source failures are collected into [`DryRunDiff::source_failures`] rather
/// than causing an error return.
pub async fn preview(
    pool: &PgPool,
    config: &Config,
    manifestation_id: Uuid,
) -> anyhow::Result<DryRunDiff> {
    let (snapshot, runs) = fan_out_for_dry_run(pool, config, manifestation_id).await?;

    let mut would_apply = Vec::new();
    let mut would_stage = Vec::new();
    let mut locked = Vec::new();
    let mut source_failures = Vec::new();

    // Aggregate results per field across sources.
    let mut per_field: HashMap<String, Vec<(String, SourceResult, PolicyInputRow)>> =
        HashMap::new();
    for run in &runs {
        match &run.outcome {
            Ok(results) => {
                for sr in results {
                    let hash = value_hash::value_hash(&sr.field_name, &sr.raw_value);
                    let row = PolicyInputRow {
                        id: Uuid::nil(),
                        value_hash: hash,
                    };
                    per_field.entry(sr.field_name.clone()).or_default().push((
                        run.source_id.clone(),
                        sr.clone(),
                        row,
                    ));
                }
            }
            Err(e) => source_failures.push(SourceFailureSummary {
                source_id: run.source_id.clone(),
                error: e.to_string(),
            }),
        }
    }

    for (field, rows) in &per_field {
        let is_locked = crate::services::enrichment::field_lock::is_locked(
            pool,
            manifestation_id,
            if matches!(field.as_str(), "title" | "description" | "language") {
                crate::services::enrichment::field_lock::EntityType::Work
            } else {
                crate::services::enrichment::field_lock::EntityType::Manifestation
            },
            field,
        )
        .await?;

        if is_locked {
            locked.push(field.clone());
            continue;
        }

        let canonical_empty = snapshot.canonical.is_empty_for(field);
        let existing_pending =
            load_existing_pending_readonly(pool, manifestation_id, field).await?;

        for (source_id, sr, incoming) in rows {
            let quorum = u32::try_from(
                rows.iter()
                    .filter(|(_, _, r)| r.value_hash == incoming.value_hash)
                    .count(),
            )
            .unwrap_or(u32::MAX);
            let mut pending_set: Vec<PolicyInputRow> = existing_pending.clone();
            for (_, _, other) in rows {
                if other.value_hash != incoming.value_hash {
                    pending_set.push(other.clone());
                }
            }
            let decision = policy::decide(field, canonical_empty, incoming, false, &pending_set);
            let change = FieldChange {
                field_name: field.clone(),
                source_id: source_id.clone(),
                new_value: sr.raw_value.clone(),
                quorum,
            };
            match decision {
                Decision::Apply(_) => {
                    would_apply.push(change);
                    break; // Only record one apply per field per run.
                }
                Decision::Stage => would_stage.push(change),
                Decision::NoOp => {}
            }
        }
    }

    Ok(DryRunDiff {
        manifestation_id,
        work_id: snapshot.work_id,
        would_apply,
        would_stage,
        locked,
        source_failures,
    })
}

async fn load_existing_pending_readonly(
    pool: &PgPool,
    manifestation_id: Uuid,
    field: &str,
) -> sqlx::Result<Vec<PolicyInputRow>> {
    let rows = sqlx::query!(
        "SELECT id, value_hash FROM metadata_versions \
         WHERE manifestation_id = $1 AND field_name = $2 AND status = 'pending'",
        manifestation_id,
        field,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PolicyInputRow {
            id: r.id,
            value_hash: r.value_hash,
        })
        .collect())
}

//! Dry-run preview for the enrichment pipeline.
//!
//! Reuses the source fan-out and cache write steps from
//! [`super::orchestrator`] but does NOT touch `metadata_versions` or
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

#[derive(Debug, Serialize)]
pub struct DryRunDiff {
    pub manifestation_id: Uuid,
    pub work_id: Uuid,
    pub would_apply: Vec<FieldChange>,
    pub would_stage: Vec<FieldChange>,
    pub locked: Vec<String>,
    pub source_failures: Vec<SourceFailureSummary>,
}

#[derive(Debug, Serialize)]
pub struct FieldChange {
    pub field_name: String,
    pub source_id: String,
    pub new_value: serde_json::Value,
    pub quorum: u32,
}

#[derive(Debug, Serialize)]
pub struct SourceFailureSummary {
    pub source_id: String,
    pub error: String,
}

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
            let quorum = rows
                .iter()
                .filter(|(_, _, r)| r.value_hash == incoming.value_hash)
                .count() as u32;
            let mut pending_set: Vec<PolicyInputRow> = existing_pending.clone();
            for (_, _, other) in rows.iter() {
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
    let rows: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
        "SELECT id, value_hash FROM metadata_versions \
         WHERE manifestation_id = $1 AND field_name = $2 AND status = 'pending'",
    )
    .bind(manifestation_id)
    .bind(field)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, value_hash)| PolicyInputRow { id, value_hash })
        .collect())
}

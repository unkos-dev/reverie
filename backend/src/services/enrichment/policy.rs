//! Field-level policy engine for the metadata enrichment pipeline.
//!
//! Determines whether an incoming `metadata_versions` row should be applied
//! (become canonical), staged (remain pending for review), or no-op'd
//! (the field is locked by the user).

// Phase B building block: callers are wired in Phase C.
#![allow(dead_code)]

use uuid::Uuid;

/// Controls how a field responds to new incoming metadata.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FieldPolicy {
    /// Automatically apply when the canonical slot is empty and all sources agree.
    AutoFill,
    /// Always stage for human review; never auto-apply.
    Propose,
    /// Field is locked by the user; all new observations are silently discarded.
    Lock,
}

/// The decision returned by [`decide`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Promote this version to canonical.  The `Uuid` is `incoming.id`.
    Apply(Uuid),
    /// Leave the row as pending (requires human approval).
    Stage,
    /// Discard the observation; do not even create a pending row.
    NoOp,
}

/// Minimal row slice that [`decide`] needs; callers build these from DB rows.
#[derive(Debug, Clone)]
pub struct PolicyInputRow {
    pub id: Uuid,
    pub value_hash: Vec<u8>,
}

/// Return the default [`FieldPolicy`] for a field name.
///
/// Unknown fields default to [`FieldPolicy::Propose`] (conservative).
pub fn default_policy(field: &str) -> FieldPolicy {
    match field {
        "title" | "sort_title" | "language" | "isbn_10" | "isbn_13" | "publisher" | "pub_date"
        | "cover" => FieldPolicy::AutoFill,
        "description" | "series" | "series_position" | "creators" | "subjects" | "genres"
        | "tags" => FieldPolicy::Propose,
        _ => FieldPolicy::Propose,
    }
}

/// Decide what to do with an incoming metadata observation.
///
/// Decision logic (evaluated in order):
/// 1. `field_locked` → [`Decision::NoOp`].
/// 2. Look up `default_policy(field)` → base policy.
/// 3. If base is [`FieldPolicy::AutoFill`] and any row in `existing_pending`
///    disagrees (different `value_hash`) → downgrade to [`FieldPolicy::Propose`].
/// 4. Dispatch:
///    - AutoFill + `canonical_is_empty` → [`Decision::Apply`].
///    - AutoFill + canonical already set → [`Decision::Stage`].
///    - Propose → [`Decision::Stage`].
///    - Lock → [`Decision::NoOp`] (unreachable here, handled in step 1).
pub fn decide(
    field: &str,
    canonical_is_empty: bool,
    incoming: &PolicyInputRow,
    field_locked: bool,
    existing_pending: &[PolicyInputRow],
) -> Decision {
    if field_locked {
        return Decision::NoOp;
    }

    let mut policy = default_policy(field);

    // Downgrade AutoFill to Propose if any pending row disagrees.
    if policy == FieldPolicy::AutoFill {
        let disagreement = existing_pending
            .iter()
            .any(|p| p.value_hash != incoming.value_hash);
        if disagreement {
            policy = FieldPolicy::Propose;
        }
    }

    match policy {
        FieldPolicy::AutoFill if canonical_is_empty => Decision::Apply(incoming.id),
        FieldPolicy::AutoFill => Decision::Stage,
        FieldPolicy::Propose => Decision::Stage,
        FieldPolicy::Lock => Decision::NoOp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(hash: &[u8]) -> PolicyInputRow {
        PolicyInputRow {
            id: Uuid::new_v4(),
            value_hash: hash.to_vec(),
        }
    }

    #[test]
    fn locked_field_is_noop() {
        let incoming = row(b"abc");
        let result = decide("title", true, &incoming, true, &[]);
        assert_eq!(result, Decision::NoOp);
    }

    #[test]
    fn locked_overrides_even_autofill_with_empty_canonical() {
        let incoming = row(b"abc");
        let result = decide("title", true, &incoming, true, &[]);
        assert_eq!(result, Decision::NoOp);
    }

    #[test]
    fn autofill_empty_canonical_applies() {
        let incoming = row(b"abc");
        let id = incoming.id;
        let result = decide("title", true, &incoming, false, &[]);
        assert_eq!(result, Decision::Apply(id));
    }

    #[test]
    fn autofill_canonical_already_set_stages() {
        let incoming = row(b"abc");
        let result = decide("title", false, &incoming, false, &[]);
        assert_eq!(result, Decision::Stage);
    }

    #[test]
    fn autofill_disagreement_downgrades_to_stage() {
        let incoming = row(b"abc");
        let pending = row(b"xyz"); // different hash
        let result = decide("title", true, &incoming, false, &[pending]);
        assert_eq!(result, Decision::Stage);
    }

    #[test]
    fn autofill_agreement_still_applies_when_empty() {
        let incoming = row(b"abc");
        let pending = row(b"abc"); // same hash — agreement
        let id = incoming.id;
        let result = decide("title", true, &incoming, false, &[pending]);
        assert_eq!(result, Decision::Apply(id));
    }

    #[test]
    fn propose_field_always_stages() {
        let incoming = row(b"abc");
        // canonical empty and no pending — still Stage because policy is Propose
        let result = decide("description", true, &incoming, false, &[]);
        assert_eq!(result, Decision::Stage);
    }

    #[test]
    fn unknown_field_defaults_to_propose_semantics() {
        let incoming = row(b"abc");
        let result = decide("foobar_unknown_field", true, &incoming, false, &[]);
        assert_eq!(result, Decision::Stage);
    }

    #[test]
    fn all_autofill_fields_recognised() {
        for field in &[
            "title",
            "sort_title",
            "language",
            "isbn_10",
            "isbn_13",
            "publisher",
            "pub_date",
            "cover",
        ] {
            assert_eq!(
                default_policy(field),
                FieldPolicy::AutoFill,
                "expected AutoFill for field '{field}'"
            );
        }
    }

    #[test]
    fn all_propose_fields_recognised() {
        for field in &[
            "description",
            "series",
            "series_position",
            "creators",
            "subjects",
            "genres",
            "tags",
        ] {
            assert_eq!(
                default_policy(field),
                FieldPolicy::Propose,
                "expected Propose for field '{field}'"
            );
        }
    }
}

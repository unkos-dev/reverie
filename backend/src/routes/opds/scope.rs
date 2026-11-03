//! URL-based scope for OPDS feeds.
//!
//! A device paired at `/opds/library/*` sees
//! the whole library (further filtered by RLS for child accounts); a device
//! paired at `/opds/shelves/{id}/*` sees only that shelf. Scope is determined
//! entirely by the feed URL — there is no per-token or per-user preference.
//!
//! ALWAYS applied inside [`crate::db::acquire_with_rls`]. Never a substitute
//! for RLS — child accounts still see only shelf-assigned manifestations
//! underneath library scope.

use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

/// URL-derived scope for an OPDS feed request.
///
/// Determined entirely by the feed URL — no per-token or per-user
/// preference. Always applied inside [`crate::db::acquire_with_rls`];
/// never a substitute for RLS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Whole library; child accounts still see only shelf-assigned
    /// manifestations under this scope via RLS.
    Library,
    /// Single shelf (by id); restricts visibility to that shelf's items.
    Shelf(Uuid),
}

/// Push the scope predicate onto an existing [`QueryBuilder`] using the given
/// manifestations alias.
///
/// For [`Scope::Library`] pushes nothing (caller is
/// responsible for any surrounding boolean glue — check `is_library` first if
/// the scope is optional).
///
/// Returning a `(String, Vec<Uuid>)` pair would NOT compose with
/// [`QueryBuilder::push_bind`]'s managed placeholder numbering — the caller
/// would have to hand-number `$N` in the embedded fragment to stay consistent
/// with the other binds. Pushing fragments + binds directly through the
/// caller's builder keeps all numbering in one place.
pub fn push_scope(qb: &mut QueryBuilder<Postgres>, scope: &Scope, manifestation_alias: &str) {
    if let Scope::Shelf(uuid) = scope {
        qb.push("EXISTS (SELECT 1 FROM shelf_items si JOIN shelves s ON s.id = si.shelf_id WHERE si.manifestation_id = ");
        qb.push(manifestation_alias);
        qb.push(".id AND s.id = ");
        qb.push_bind(*uuid);
        qb.push(" AND s.user_id = current_setting('app.current_user_id', true)::uuid)");
    }
}

/// Push an `EXISTS (SELECT 1 FROM manifestations m WHERE m.work_id =
/// {parent_alias}.work_id … )` fragment to use on navigation feeds keyed on a
/// parent table (works, authors, series).
///
/// When scope is a shelf, the
/// `manifestations` row must also be in that shelf.
pub fn push_visible_manifestation(
    qb: &mut QueryBuilder<Postgres>,
    scope: &Scope,
    parent_alias_for_work_id_column: &str,
) {
    qb.push("EXISTS (SELECT 1 FROM manifestations m WHERE m.work_id = ");
    qb.push(parent_alias_for_work_id_column);
    qb.push(".work_id");
    if !matches!(scope, Scope::Library) {
        qb.push(" AND ");
        push_scope(qb, scope, "m");
    }
    qb.push(")");
}

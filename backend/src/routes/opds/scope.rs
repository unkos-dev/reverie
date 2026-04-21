//! URL-based scope for OPDS feeds. A device paired at `/opds/library/*` sees
//! the whole library (further filtered by RLS for child accounts); a device
//! paired at `/opds/shelves/{id}/*` sees only that shelf. Scope is determined
//! entirely by the feed URL — there is no per-token or per-user preference.
//!
//! ALWAYS applied inside [`crate::db::acquire_with_rls`]. Never a substitute
//! for RLS — child accounts still see only shelf-assigned manifestations
//! underneath library scope.

use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Library,
    Shelf(Uuid),
}

/// Push the scope predicate onto an existing [`QueryBuilder`] using the given
/// manifestations alias. For [`Scope::Library`] pushes nothing (caller is
/// responsible for any surrounding boolean glue — check `is_library` first if
/// the scope is optional).
///
/// Returning a `(String, Vec<Uuid>)` pair would NOT compose with
/// [`QueryBuilder::push_bind`]'s managed placeholder numbering — the caller
/// would have to hand-number `$N` in the embedded fragment to stay consistent
/// with the other binds. Pushing fragments + binds directly through the
/// caller's builder keeps all numbering in one place.
pub fn push_scope(qb: &mut QueryBuilder<'_, Postgres>, scope: &Scope, manifestation_alias: &str) {
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
/// parent table (works, authors, series). When scope is a shelf, the
/// `manifestations` row must also be in that shelf.
#[allow(dead_code)] // alternative helper; current handlers inline the EXISTS
pub fn push_visible_manifestation(
    qb: &mut QueryBuilder<'_, Postgres>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_scope_pushes_nothing() {
        let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new("SELECT 1 WHERE ");
        push_scope(&mut qb, &Scope::Library, "m");
        let sql = qb.into_sql();
        assert_eq!(sql, "SELECT 1 WHERE ");
    }

    #[test]
    fn shelf_scope_pushes_fragment_and_one_bind() {
        let shelf_id = Uuid::new_v4();
        let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new("SELECT 1 WHERE ");
        push_scope(&mut qb, &Scope::Shelf(shelf_id), "m");
        let sql = qb.into_sql();
        assert!(sql.contains("shelf_items"));
        assert!(sql.contains("JOIN shelves s ON"));
        assert!(sql.contains("s.user_id = current_setting('app.current_user_id', true)::uuid"));
        // exactly one $N — QueryBuilder assigns $1 for the first push_bind.
        assert!(sql.contains("s.id = $1"));
        // No second bind.
        assert!(!sql.contains("$2"));
    }

    #[test]
    fn visible_manifestation_library_is_plain_exists() {
        let mut qb: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT 1 FROM authors a WHERE ");
        push_visible_manifestation(&mut qb, &Scope::Library, "a");
        let sql = qb.into_sql();
        assert!(
            sql.contains("EXISTS (SELECT 1 FROM manifestations m WHERE m.work_id = a.work_id)")
        );
        assert!(!sql.contains("shelf_items"));
    }

    #[test]
    fn visible_manifestation_shelf_wraps_scope() {
        let shelf_id = Uuid::new_v4();
        let mut qb: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT 1 FROM authors a WHERE ");
        push_visible_manifestation(&mut qb, &Scope::Shelf(shelf_id), "a");
        let sql = qb.into_sql();
        assert!(sql.contains("m.work_id = a.work_id"));
        assert!(sql.contains("shelf_items"));
        assert!(sql.contains("s.id = $1"));
    }
}

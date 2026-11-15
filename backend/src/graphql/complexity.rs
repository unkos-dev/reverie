use crate::{error::AppError, services::reader, state::AppState};
use async_graphql::extensions::{Extension, ExtensionContext, ExtensionFactory, NextValidation};
use async_graphql::{ServerError, ServerResult, ValidationResult};
use std::sync::Arc;

pub(super) const LIMIT: usize = 82_568;
pub(super) const DEPTH: usize = 12;

tokio::task_local! {
    static PAGE_CEILING: Option<u32>;
}

pub(super) struct Defaults;
impl ExtensionFactory for Defaults {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(PageDefaults)
    }
}
struct PageDefaults;
#[async_trait::async_trait]
impl Extension for PageDefaults {
    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> Result<ValidationResult, Vec<ServerError>> {
        // THREAT: validation uses this request's page ceiling, never another request's default.
        let ceiling = ctx
            .data_opt::<AppState>()
            .map(|state| state.config.opds.page_size);
        PAGE_CEILING.scope(ceiling, next.run(ctx)).await
    }
}

pub(super) fn effective_page(first: Option<i32>, ceiling: u32) -> Result<i32, AppError> {
    let first = first.map_or_else(|| i64::from(ceiling), i64::from);
    if !(0..=i64::from(ceiling)).contains(&first) {
        return Err(AppError::Validation("Invalid page size.".into()));
    }
    i32::try_from(first).map_err(|e| AppError::Internal(e.into()))
}

pub(super) fn page(first: Option<i32>, child: usize) -> ServerResult<usize> {
    let ceiling = PAGE_CEILING
        .try_with(|ceiling| *ceiling)
        .map_err(|_| ServerError::new("Page configuration is unavailable.", None))?
        .ok_or_else(|| ServerError::new("Page configuration is unavailable.", None))?;
    let size = effective_page(first, ceiling).map_err(|error| {
        super::safe_error(error).into_server_error(async_graphql::Pos::default())
    })?;
    weighted(usize::try_from(size).map_err(|_| excessive())?, child)
}

pub(super) fn suggestions(limit: Option<i32>, child: usize) -> ServerResult<usize> {
    let size = reader::suggestion_limit(limit.map(i64::from));
    weighted(usize::try_from(size).map_err(|_| excessive())?, child)
}

pub(super) fn labels(size: usize, child: usize) -> ServerResult<usize> {
    reader::validate_label_count(size).map_err(|error| {
        super::safe_error(error).into_server_error(async_graphql::Pos::default())
    })?;
    weighted(size, child)
}

pub(super) fn weighted(size: usize, child: usize) -> ServerResult<usize> {
    let cost = size
        .checked_mul(child)
        .and_then(|cost| cost.checked_add(1))
        .ok_or_else(excessive)?;
    if cost > LIMIT {
        return Err(excessive());
    }
    Ok(cost)
}
fn excessive() -> ServerError {
    ServerError::new("Query is too complex.", None)
}

pub(super) fn nested_size(
    kind: reader::RelationKind,
    first: Option<i32>,
    ceiling: u32,
) -> Result<i32, AppError> {
    use reader::RelationKind;
    let (default, maximum) = match kind {
        RelationKind::SeriesWorks | RelationKind::ShelfItems => {
            return effective_page(first, ceiling);
        }
        RelationKind::Manifestations => (2, 20),
        RelationKind::Tags => (10, 50),
        RelationKind::Authors
        | RelationKind::Contributors
        | RelationKind::WorkIdentifiers
        | RelationKind::Genres
        | RelationKind::Moods
        | RelationKind::Identifiers
        | RelationKind::Ratings => (5, 20),
    };
    let default =
        i32::try_from(default.min(ceiling)).map_err(|error| AppError::Internal(error.into()))?;
    effective_page(first.or(Some(default)), maximum.min(ceiling))
}

pub(super) fn nested(
    kind: reader::RelationKind,
    first: Option<i32>,
    child: usize,
) -> ServerResult<usize> {
    let ceiling = PAGE_CEILING
        .try_with(|ceiling| *ceiling)
        .map_err(|_| ServerError::new("Page configuration is unavailable.", None))?
        .ok_or_else(|| ServerError::new("Page configuration is unavailable.", None))?;
    let size = nested_size(kind, first, ceiling).map_err(|error| {
        super::safe_error(error).into_server_error(async_graphql::Pos::default())
    })?;
    weighted(usize::try_from(size).map_err(|_| excessive())?, child)
}

pub(super) fn pending(first: Option<i32>, child: usize) -> ServerResult<usize> {
    let ceiling = PAGE_CEILING
        .try_with(|ceiling| *ceiling)
        .map_err(|_| ServerError::new("Page configuration is unavailable.", None))?
        .ok_or_else(|| ServerError::new("Page configuration is unavailable.", None))?;
    let size = pending_size(first, ceiling).map_err(|error| {
        super::safe_error(error).into_server_error(async_graphql::Pos::default())
    })?;
    weighted(usize::try_from(size).map_err(|_| excessive())?, child)
}

pub(super) fn pending_size(first: Option<i32>, ceiling: u32) -> Result<i32, AppError> {
    let default = i32::try_from(20.min(ceiling)).map_err(|e| AppError::Internal(e.into()))?;
    effective_page(first.or(Some(default)), 50.min(ceiling))
}

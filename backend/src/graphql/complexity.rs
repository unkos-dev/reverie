use crate::{error::AppError, services::reader};
use async_graphql::{ServerError, ServerResult};

pub(super) const LIMIT: usize = 82_568;
pub(super) const DEPTH: usize = 12;

pub(super) fn effective_page(first: i32, ceiling: u32) -> Result<i32, AppError> {
    if !(0..=i64::from(ceiling)).contains(&i64::from(first)) {
        return Err(AppError::Validation("Invalid page size.".into()));
    }
    Ok(first)
}

pub(super) fn page(first: i32, child: usize) -> ServerResult<usize> {
    let size = effective_page(first, 100).map_err(|error| {
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

pub(super) fn nested_size(kind: reader::RelationKind, first: i32) -> Result<i32, AppError> {
    use reader::RelationKind;
    let maximum = match kind {
        RelationKind::SeriesWorks | RelationKind::ShelfItems => 100,
        RelationKind::Tags => 50,
        _ => 20,
    };
    effective_page(first, maximum)
}

pub(super) fn nested(kind: reader::RelationKind, first: i32, child: usize) -> ServerResult<usize> {
    let size = nested_size(kind, first).map_err(|error| {
        super::safe_error(error).into_server_error(async_graphql::Pos::default())
    })?;
    weighted(usize::try_from(size).map_err(|_| excessive())?, child)
}

pub(super) fn pending(first: i32, child: usize) -> ServerResult<usize> {
    let size = pending_size(first).map_err(|error| {
        super::safe_error(error).into_server_error(async_graphql::Pos::default())
    })?;
    weighted(usize::try_from(size).map_err(|_| excessive())?, child)
}

pub(super) fn pending_size(first: i32) -> Result<i32, AppError> {
    effective_page(first, 50)
}

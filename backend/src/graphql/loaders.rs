use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::dataloader::{DataLoader, HashMapCache, Loader};
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::services::reader::{
    self, BookRecord, RelationKey, RelationPage, SeriesRecord, ShelfRecord, WorkRecord,
};
use crate::state::AppState;

pub(super) struct ReaderLoader {
    pub(super) user: CurrentUser,
    pub(super) state: AppState,
}
pub(super) type RequestLoader = DataLoader<ReaderLoader, HashMapCache>;

pub(super) fn request_loader(user: CurrentUser, state: AppState) -> RequestLoader {
    DataLoader::with_cache(
        ReaderLoader { user, state },
        tokio::spawn,
        HashMapCache::default(),
    )
}

impl Loader<Uuid> for ReaderLoader {
    type Value = BookRecord;
    type Error = Arc<AppError>;
    async fn load(&self, keys: &[Uuid]) -> Result<HashMap<Uuid, Self::Value>, Self::Error> {
        reader::book_records(&self.user, &self.state, keys)
            .await
            .map_err(Arc::new)
    }
}

macro_rules! domain_loader {
    ($key:ident, $value:ty, $operation:ident) => {
        #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
        pub(super) struct $key(pub(super) Uuid);
        impl Loader<$key> for ReaderLoader {
            type Value = Arc<$value>;
            type Error = Arc<AppError>;
            async fn load(&self, keys: &[$key]) -> Result<HashMap<$key, Self::Value>, Self::Error> {
                let ids: Vec<_> = keys.iter().map(|key| key.0).collect();
                reader::$operation(&self.user, &self.state, &ids)
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|(id, row)| ($key(id), Arc::new(row)))
                            .collect()
                    })
                    .map_err(Arc::new)
            }
        }
    };
}

domain_loader!(WorkKey, WorkRecord, work_records);
domain_loader!(ShelfKey, ShelfRecord, shelf_records);
domain_loader!(SeriesKey, SeriesRecord, series_records);

impl Loader<RelationKey> for ReaderLoader {
    type Value = RelationPage;
    type Error = Arc<AppError>;
    async fn load(
        &self,
        keys: &[RelationKey],
    ) -> Result<HashMap<RelationKey, Self::Value>, Self::Error> {
        reader::relation_pages(&self.user, &self.state, keys)
            .await
            .map_err(Arc::new)
    }
}

pub(super) fn batch_error(error: &Arc<AppError>) -> async_graphql::Error {
    if let AppError::Validation(detail) = error.as_ref() {
        return super::safe_error(AppError::Validation(detail.clone()));
    }
    tracing::error!(error = ?error, "Reader batch failed");
    super::safe_error(AppError::Internal(anyhow::anyhow!("Reader batch failed")))
}

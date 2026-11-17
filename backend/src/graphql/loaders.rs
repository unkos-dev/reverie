use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use std::sync::{Mutex, MutexGuard};

use tokio::sync::Notify;
use uuid::Uuid;

use super::{GraphError, GraphResult, safe_error};
use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::services::metadata::editing::{self, MetadataSnapshot, SnapshotKey};
use crate::services::reader::{
    self, BookRecord, PendingKey, PendingPage, RelationKey, RelationPage, SeriesRecord,
    ShelfRecord, VersionSet, WorkRecord,
};
use crate::state::AppState;

const DISPATCH_YIELDS: usize = 4;

type Loaded<V> = Result<Option<V>, Arc<AppError>>;

struct Batch<K, V> {
    resolved: HashMap<K, Loaded<V>>,
    queued: Vec<K>,
}

struct Batcher<K, V> {
    batch: Mutex<Batch<K, V>>,
    settled: Notify,
}

impl<K: Clone + Eq + Hash + Send + Sync, V: Clone + Send + Sync> Batcher<K, V> {
    fn new() -> Self {
        Self {
            batch: Mutex::new(Batch {
                resolved: HashMap::new(),
                queued: Vec::new(),
            }),
            settled: Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Batch<K, V>> {
        self.batch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn prime(&self, values: impl IntoIterator<Item = (K, V)>) {
        let mut batch = self.lock();
        for (key, value) in values {
            batch.resolved.insert(key, Ok(Some(value)));
        }
    }

    async fn load<F, Fut>(&self, keys: &[K], fetch: F) -> GraphResult<HashMap<K, V>>
    where
        F: FnOnce(Vec<K>) -> Fut + Send,
        Fut: Future<Output = Result<HashMap<K, V>, AppError>> + Send,
    {
        {
            let mut batch = self.lock();
            if keys.iter().all(|key| batch.resolved.contains_key(key)) {
                return collect(&batch.resolved, keys);
            }
            for key in keys {
                if !batch.resolved.contains_key(key) && !batch.queued.contains(key) {
                    batch.queued.push(key.clone());
                }
            }
        }
        for _ in 0..DISPATCH_YIELDS {
            tokio::task::yield_now().await;
        }
        let mut fetch = Some(fetch);
        loop {
            let settled = self.settled.notified();
            tokio::pin!(settled);
            settled.as_mut().enable();
            let dispatch = {
                let mut batch = self.lock();
                if keys.iter().all(|key| batch.resolved.contains_key(key)) {
                    return collect(&batch.resolved, keys);
                }
                if keys.iter().any(|key| batch.queued.contains(key)) {
                    Some(std::mem::take(&mut batch.queued))
                } else {
                    None
                }
            };
            match (dispatch, fetch.take()) {
                (Some(queued), Some(fetch)) => {
                    let outcome = fetch(queued.clone()).await.map_err(Arc::new);
                    let mut batch = self.lock();
                    for key in queued {
                        let loaded = match &outcome {
                            Ok(rows) => Ok(rows.get(&key).cloned()),
                            Err(error) => Err(Arc::clone(error)),
                        };
                        batch.resolved.insert(key, loaded);
                    }
                    let values = collect(&batch.resolved, keys);
                    drop(batch);
                    self.settled.notify_waiters();
                    return values;
                }
                (_, unused) => {
                    fetch = unused;
                    settled.await;
                }
            }
        }
    }
}

fn collect<K: Clone + Eq + Hash, V: Clone>(
    resolved: &HashMap<K, Loaded<V>>,
    keys: &[K],
) -> GraphResult<HashMap<K, V>> {
    let mut values = HashMap::with_capacity(keys.len());
    for key in keys {
        match resolved.get(key) {
            Some(Ok(Some(value))) => {
                values.insert(key.clone(), value.clone());
            }
            Some(Err(error)) => return Err(batch_error(error)),
            Some(Ok(None)) | None => {}
        }
    }
    Ok(values)
}

fn batch_error(error: &Arc<AppError>) -> GraphError {
    if let AppError::Validation(detail) = error.as_ref() {
        return safe_error(AppError::Validation(detail.clone()));
    }
    tracing::error!(error = ?error, "Reader batch failed");
    safe_error(AppError::Internal(anyhow::anyhow!("Reader batch failed")))
}

fn shared<V>(rows: HashMap<Uuid, V>) -> HashMap<Uuid, Arc<V>> {
    rows.into_iter()
        .map(|(id, row)| (id, Arc::new(row)))
        .collect()
}

pub(super) struct Loaders {
    user: CurrentUser,
    state: AppState,
    books: Batcher<Uuid, BookRecord>,
    works: Batcher<Uuid, Arc<WorkRecord>>,
    shelves: Batcher<Uuid, Arc<ShelfRecord>>,
    series: Batcher<Uuid, Arc<SeriesRecord>>,
    relations: Batcher<RelationKey, RelationPage>,
    snapshots: Batcher<SnapshotKey, Arc<MetadataSnapshot>>,
    versions: Batcher<Uuid, Arc<VersionSet>>,
    canonical: Batcher<Uuid, i32>,
    pending: Batcher<PendingKey, PendingPage>,
}

impl Loaders {
    pub(super) fn new(user: CurrentUser, state: AppState) -> Self {
        Self {
            user,
            state,
            books: Batcher::new(),
            works: Batcher::new(),
            shelves: Batcher::new(),
            series: Batcher::new(),
            relations: Batcher::new(),
            snapshots: Batcher::new(),
            versions: Batcher::new(),
            canonical: Batcher::new(),
            pending: Batcher::new(),
        }
    }

    pub(super) async fn books(&self, ids: &[Uuid]) -> GraphResult<HashMap<Uuid, BookRecord>> {
        self.books
            .load(ids, |keys| async move {
                reader::book_records(&self.user, &self.state, &keys).await
            })
            .await
    }

    pub(super) async fn book(&self, id: Uuid) -> GraphResult<Option<BookRecord>> {
        Ok(self.books(&[id]).await?.remove(&id))
    }

    pub(super) fn prime_books(&self, records: impl IntoIterator<Item = (Uuid, BookRecord)>) {
        self.books.prime(records);
    }

    pub(super) async fn works(&self, ids: &[Uuid]) -> GraphResult<HashMap<Uuid, Arc<WorkRecord>>> {
        self.works
            .load(ids, |keys| async move {
                reader::work_records(&self.user, &self.state, &keys)
                    .await
                    .map(shared)
            })
            .await
    }

    pub(super) async fn work(&self, id: Uuid) -> GraphResult<Option<Arc<WorkRecord>>> {
        Ok(self.works(&[id]).await?.remove(&id))
    }

    pub(super) async fn shelf(&self, id: Uuid) -> GraphResult<Option<Arc<ShelfRecord>>> {
        let mut rows = self
            .shelves
            .load(&[id], |keys| async move {
                reader::shelf_records(&self.user, &self.state, &keys)
                    .await
                    .map(shared)
            })
            .await?;
        Ok(rows.remove(&id))
    }

    pub(super) fn prime_shelves(
        &self,
        records: impl IntoIterator<Item = (Uuid, Arc<ShelfRecord>)>,
    ) {
        self.shelves.prime(records);
    }

    pub(super) async fn series(&self, id: Uuid) -> GraphResult<Option<Arc<SeriesRecord>>> {
        let mut rows = self
            .series
            .load(&[id], |keys| async move {
                reader::series_records(&self.user, &self.state, &keys)
                    .await
                    .map(shared)
            })
            .await?;
        Ok(rows.remove(&id))
    }

    pub(super) async fn relation(&self, key: RelationKey) -> GraphResult<Option<RelationPage>> {
        let mut rows = self
            .relations
            .load(std::slice::from_ref(&key), |keys| async move {
                reader::relation_pages(&self.user, &self.state, &keys).await
            })
            .await?;
        Ok(rows.remove(&key))
    }

    pub(super) async fn snapshot(
        &self,
        key: SnapshotKey,
    ) -> GraphResult<Option<Arc<MetadataSnapshot>>> {
        let mut rows = self
            .snapshots
            .load(std::slice::from_ref(&key), |keys| async move {
                editing::snapshot_pages(&self.user, &self.state, &keys)
                    .await
                    .map(|rows| {
                        rows.into_iter()
                            .map(|(key, value)| (key, Arc::new(value)))
                            .collect()
                    })
            })
            .await?;
        Ok(rows.remove(&key))
    }

    pub(super) async fn versions(&self, id: Uuid) -> GraphResult<Option<Arc<VersionSet>>> {
        let mut rows = self
            .versions
            .load(&[id], |keys| async move {
                reader::versions(&self.user, &self.state, &keys)
                    .await
                    .map(shared)
            })
            .await?;
        Ok(rows.remove(&id))
    }

    pub(super) async fn canonical_count(&self, id: Uuid) -> GraphResult<Option<i32>> {
        let mut rows = self
            .canonical
            .load(&[id], |keys| async move {
                reader::canonical_counts(&self.user, &self.state, &keys).await
            })
            .await?;
        Ok(rows.remove(&id))
    }

    pub(super) async fn pending(&self, key: PendingKey) -> GraphResult<Option<PendingPage>> {
        let mut rows = self
            .pending
            .load(std::slice::from_ref(&key), |keys| async move {
                reader::pending_pages(&self.user, &self.state, &keys).await
            })
            .await?;
        Ok(rows.remove(&key))
    }
}

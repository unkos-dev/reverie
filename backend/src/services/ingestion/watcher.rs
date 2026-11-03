//! `notify`-based filesystem watcher for the ingestion drop-zone.
//!
//! Wraps the platform-specific [`notify::RecommendedWatcher`] (inotify on `Linux`,
//! FSEvents on `macOS`) and adds a 2-second debounce window. Rapid bursts of
//! filesystem events — common when large files are copied — are coalesced into a
//! single batch so the orchestrator is not triggered before writes complete.
//!
//! Deletion and rename events bypass the delay. The coordinator rechecks names
//! through ingestion authority; event kinds do not establish file existence.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Watch `ingestion_path` for new/modified files and send batches via `tx`.
///
/// Debounces events: after the last filesystem event, waits 2 seconds of quiet
/// before sending the accumulated paths as a batch. Exits cleanly when `cancel`
/// is triggered.
///
/// # Errors
///
/// Returns `anyhow::Error` if the underlying `notify` watcher cannot be created
/// (e.g. the platform watcher is unavailable) or if `ingestion_path` cannot be
/// watched (e.g. the directory does not exist or permissions are denied), or when
/// the underlying watcher reports a runtime error.
pub async fn watch(
    ingestion_path: PathBuf,
    tx: mpsc::Sender<Vec<PathBuf>>,
    cancel: CancellationToken,
) -> Result<(), anyhow::Error> {
    register(&ingestion_path, tx, cancel)?.await
}

pub(crate) fn register(
    ingestion_path: &std::path::Path,
    tx: mpsc::Sender<Vec<PathBuf>>,
    cancel: CancellationToken,
) -> anyhow::Result<impl std::future::Future<Output = anyhow::Result<()>> + Send + use<>> {
    let (notify_tx, mut notify_rx) =
        mpsc::channel::<Result<(Vec<PathBuf>, bool), notify::Error>>(64);

    let mut watcher = {
        RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let change = match res {
                    Ok(event) => {
                        let immediate = matches!(
                            event.kind,
                            EventKind::Remove(_)
                                | EventKind::Modify(notify::event::ModifyKind::Name(_))
                        );
                        if !matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) || event.paths.is_empty()
                        {
                            return;
                        }
                        Ok((event.paths, immediate))
                    }
                    Err(error) => Err(error),
                };
                if let Err(error) = notify_tx.blocking_send(change) {
                    tracing::warn!(%error, "watcher event channel closed");
                }
            },
            notify::Config::default(),
        )?
    };

    watcher.watch(ingestion_path, RecursiveMode::Recursive)?;
    tracing::info!(path = %ingestion_path.display(), "ingestion watcher started");

    Ok(async move {
        let mut pending: Vec<PathBuf> = Vec::new();
        let debounce = tokio::time::Duration::from_secs(2);

        loop {
            // If we have pending paths, wait for more events or debounce timeout
            if pending.is_empty() {
                // No pending paths — wait for first event or cancellation
                tokio::select! {
                    () = cancel.cancelled() => {
                        tracing::info!("watcher cancelled (idle)");
                        break;
                    }
                    Some(change) = notify_rx.recv() => {
                        let (paths, immediate) = change?;
                        pending.extend(paths);
                        if immediate && tx.send(std::mem::take(&mut pending)).await.is_err() { break; }
                    }
                }
            } else {
                tokio::select! {
                    () = cancel.cancelled() => {
                        tracing::info!("watcher cancelled, flushing pending batch");
                        if !pending.is_empty()
                            && let Err(e) = tx.send(std::mem::take(&mut pending)).await
                        {
                            tracing::warn!(error = ?e, "watcher: batch channel closed during cancellation flush");
                        }
                        break;
                    }
                    Some(change) = notify_rx.recv() => {
                        let (paths, immediate) = change?;
                        pending.extend(paths);
                        if immediate && tx.send(std::mem::take(&mut pending)).await.is_err() { break; }
                    }
                    () = tokio::time::sleep(debounce) => {
                        // Debounce complete — send batch
                        pending.sort();
                        pending.dedup();
                        let batch = std::mem::take(&mut pending);
                        tracing::info!(count = batch.len(), "watcher sending batch");
                        if tx.send(batch).await.is_err() {
                            tracing::warn!("batch receiver dropped, stopping watcher");
                            break;
                        }
                    }
                }
            }
        }

        drop(watcher);
        tracing::info!("ingestion watcher stopped");
        Ok(())
    })
}

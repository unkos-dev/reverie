//! `notify`-based filesystem watcher for the ingestion drop-zone.
//!
//! Wraps the platform-specific [`notify::RecommendedWatcher`] (inotify on `Linux`,
//! FSEvents on `macOS`) and adds a 2-second debounce window. Rapid bursts of
//! filesystem events — common when large files are copied — are coalesced into a
//! single batch so the orchestrator is not triggered before writes complete.
//!
//! Only `Create` and `Modify` events on regular files are forwarded; directory events
//! and deletions are ignored. Symlinks are not followed (`follow_links(false)` is
//! set in the orchestrator's `WalkDir` call) to prevent an attacker with write access
//! to the drop-zone from escaping the ingestion root via a crafted symlink.

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
/// watched (e.g. the directory does not exist or permissions are denied).
pub async fn watch(
    ingestion_path: PathBuf,
    tx: mpsc::Sender<Vec<PathBuf>>,
    cancel: CancellationToken,
) -> Result<(), anyhow::Error> {
    let (notify_tx, mut notify_rx) = mpsc::channel::<Vec<PathBuf>>(64);

    let mut watcher = {
        let notify_tx = notify_tx.clone();
        RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            let paths: Vec<PathBuf> =
                                event.paths.into_iter().filter(|p| p.is_file()).collect();
                            if !paths.is_empty()
                                && let Err(e) = notify_tx.blocking_send(paths)
                            {
                                tracing::warn!(error = ?e, "watcher: notify channel closed; stopping event forwarding");
                            }
                        }
                        _ => {}
                    }
                }
            },
            notify::Config::default(),
        )?
    };

    watcher.watch(&ingestion_path, RecursiveMode::Recursive)?;
    tracing::info!(path = %ingestion_path.display(), "ingestion watcher started");

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
                Some(paths) = notify_rx.recv() => {
                    pending.extend(paths);
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
                Some(paths) = notify_rx.recv() => {
                    pending.extend(paths);
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
}

#[cfg(test)]
#[expect(
    clippy::let_underscore_must_use,
    reason = "test code: discarding JoinHandle and Result in test harness scaffolding is intentional; the crate-root cfg_attr only covers unwrap_used/expect_used"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn watcher_detects_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        let watch_path = dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let _ = watch(watch_path, tx, cancel2).await;
        });

        // Give the watcher time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Create a file
        let test_file = dir.path().join("test.epub");
        std::fs::write(&test_file, b"content").unwrap();

        // Wait for the debounced batch (2s + margin)
        let batch = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout waiting for batch")
            .expect("channel closed");

        assert!(!batch.is_empty());
        assert!(batch.iter().any(|p| p.file_name().unwrap() == "test.epub"));

        cancel.cancel();
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn watcher_exits_on_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let cancel2 = cancel.clone();

        let watch_path = dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let _ = watch(watch_path, tx, cancel2).await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        cancel.cancel();

        let result = tokio::time::timeout(tokio::time::Duration::from_secs(3), handle).await;
        assert!(result.is_ok(), "watcher did not exit after cancel");
    }
}

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Watch `ingestion_path` for new/modified files and send batches via `tx`.
///
/// Debounces events: after the last filesystem event, waits 2 seconds of quiet
/// before sending the accumulated paths as a batch. Exits cleanly when `cancel`
/// is triggered.
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
                            if !paths.is_empty() {
                                let _ = notify_tx.blocking_send(paths);
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
        if !pending.is_empty() {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("watcher cancelled, flushing pending batch");
                    if !pending.is_empty() {
                        let _ = tx.send(std::mem::take(&mut pending)).await;
                    }
                    break;
                }
                Some(paths) = notify_rx.recv() => {
                    pending.extend(paths);
                }
                _ = tokio::time::sleep(debounce) => {
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
        } else {
            // No pending paths — wait for first event or cancellation
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("watcher cancelled (idle)");
                    break;
                }
                Some(paths) = notify_rx.recv() => {
                    pending.extend(paths);
                }
            }
        }
    }

    drop(watcher);
    tracing::info!("ingestion watcher stopped");
    Ok(())
}

#[cfg(test)]
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

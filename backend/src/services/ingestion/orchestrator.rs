use std::path::{Path, PathBuf};

use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::Config;
use crate::models::ingestion_status::IngestionStatus;
use crate::models::manifestation_format::ManifestationFormat;
use crate::models::storage_library::LibraryId;
use crate::models::validation_status::ValidationStatus;
use crate::models::{library_path_claim, work};
#[cfg(test)]
use crate::services::epub;
use crate::services::epub::ValidationOutcome;
use crate::services::files::{LibraryFiles, LibraryLocation, RelativeFilePath};
use crate::services::ingestion::{cleanup, copier, path_template};
use crate::services::metadata;
use crate::services::writeback::path_rename;

/// Shared command handle for the sole ingestion scheduling owner.
#[derive(Clone)]
pub struct CoordinatorHandle {
    sender: mpsc::Sender<tokio::sync::oneshot::Sender<anyhow::Result<DiscoveryResult>>>,
}

/// Classification counts from discovery, before ingestion completes.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct DiscoveryResult {
    /// Inputs ready for the coordinator's attempt queue.
    pub queued: usize,
    /// Inputs awaiting readiness or a retry deadline.
    pub deferred: usize,
    /// Inputs whose current generation is ineligible.
    pub suppressed: usize,
    /// Existing activity resource for observing attempts.
    pub monitor: &'static str,
}

pub type CoordinatorCommands =
    mpsc::Receiver<tokio::sync::oneshot::Sender<anyhow::Result<DiscoveryResult>>>;

/// Create the shared handle and its single-owner receiver.
#[must_use]
pub fn coordinator_channel() -> (CoordinatorHandle, CoordinatorCommands) {
    let (sender, receiver) = mpsc::channel(16);
    (CoordinatorHandle { sender }, receiver)
}

impl CoordinatorHandle {
    /// Discover current inputs without bypassing readiness.
    ///
    /// # Errors
    /// Returns an error when the owner or discovery is unavailable.
    pub async fn scan(&self) -> anyhow::Result<DiscoveryResult> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.sender
            .send(sender)
            .await
            .map_err(|_| anyhow::anyhow!("ingestion coordinator unavailable"))?;
        receiver
            .await
            .map_err(|_| anyhow::anyhow!("ingestion discovery interrupted"))?
    }
}

const READINESS: std::time::Duration = std::time::Duration::from_secs(10);
const RETRIES: [u64; 5] = [300, 1800, 7200, 28800, 86400];
const PROBES: [u64; 4] = [30, 60, 120, 300];
const IDLE: std::time::Duration = std::time::Duration::from_secs(120);
const STALL: std::time::Duration = std::time::Duration::from_secs(300);

struct Pause {
    step: usize,
    next: tokio::time::Instant,
}

impl Pause {
    fn new() -> Self {
        Self {
            step: 0,
            next: tokio::time::Instant::now() + std::time::Duration::from_secs(PROBES[0]),
        }
    }

    fn failed(&mut self) {
        self.step = (self.step + 1).min(PROBES.len() - 1);
        self.next = tokio::time::Instant::now() + std::time::Duration::from_secs(PROBES[self.step]);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FinalisationHealth {
    NoFreshFailure,
    SharedFailure,
}

struct PendingResult {
    input: crate::models::ingestion_input::Input,
    job: Uuid,
    result: ProcessResult,
    source_deleted: bool,
    health: FinalisationHealth,
}

struct Active {
    future: futures::future::BoxFuture<'static, PendingResult>,
    progress: copier::Progress,
    count: u64,
    transitions: u64,
    last_progress: tokio::time::Instant,
    next_warning: tokio::time::Instant,
    recommit: bool,
}

impl Active {
    fn recommit(
        mut pending: PendingResult,
        config: Config,
        pool: PgPool,
        files: LibraryFiles,
    ) -> Self {
        use futures::FutureExt;
        let progress = match &pending.result {
            ProcessResult::Accepted(accepted) => accepted.candidate.progress(),
            _ => copier::Progress::new(&CancellationToken::new()),
        };
        let now = tokio::time::Instant::now();
        let future = async move {
            if let ProcessResult::Accepted(accepted) = &mut pending.result {
                match std::panic::AssertUnwindSafe(finish_accepted(
                    accepted,
                    &config,
                    &pool,
                    &files,
                    Some((&pending.input, pending.job)),
                ))
                .catch_unwind()
                .await
                {
                    Ok(Ok(result)) => {
                        pending.health = accepted.health;
                        pending.result = result;
                    }
                    Ok(Err(error)) => tracing::warn!(%error, "accepted result recommit deferred"),
                    Err(_) => {
                        accepted.failure = Some((
                            crate::models::ingestion_input::AttemptOutcome::TransientInput,
                            "panic during finalisation".into(),
                        ));
                    }
                }
            }
            pending
        }
        .boxed();
        Self {
            future,
            progress,
            count: 0,
            transitions: 0,
            last_progress: now,
            next_warning: now + STALL,
            recommit: true,
        }
    }

    fn tick(&mut self) {
        let now = tokio::time::Instant::now();
        let count = self.progress.count();
        let transitions = self.progress.transitions();
        if count != self.count || transitions != self.transitions {
            self.count = count;
            self.transitions = transitions;
            self.last_progress = now;
            self.next_warning = now + STALL;
        }
        if self.progress.phase() == copier::Phase::Streaming
            && now.duration_since(self.last_progress) >= IDLE
        {
            self.progress.cancel.cancel();
        }
        if now >= self.next_warning {
            tracing::warn!(
                idle_seconds = now.duration_since(self.last_progress).as_secs(),
                "ingestion attempt stalled; awaiting blocking return"
            );
            self.next_warning += STALL;
        }
    }
}

struct Coordinator {
    config: Config,
    pool: PgPool,
    files: LibraryFiles,
    settings: std::sync::Arc<tokio::sync::RwLock<crate::models::settings::Settings>>,
    inputs: std::collections::HashMap<Uuid, crate::models::ingestion_input::Input>,
    deadlines: tokio_util::time::DelayQueue<Uuid>,
    keys: std::collections::HashMap<Uuid, tokio_util::time::delay_queue::Key>,
    ready: std::collections::VecDeque<Uuid>,
    ready_set: std::collections::HashSet<Uuid>,
    observed: std::collections::HashMap<
        Vec<u8>,
        (
            crate::models::ingestion_input::Fingerprint,
            tokio::time::Instant,
        ),
    >,
    pause: Option<Pause>,
    pending: Option<PendingResult>,
    lock: Option<sqlx::pool::PoolConnection<sqlx::Postgres>>,
    library_id: Option<LibraryId>,
    recovered: bool,
    discovered: bool,
    accepted: Option<Vec<String>>,
    unresolved: std::collections::HashSet<Uuid>,
}

impl Coordinator {
    fn pause(&mut self, error: &dyn std::fmt::Display) {
        if self.pause.is_none() {
            tracing::warn!(%error, "ingestion coordinator paused");
            self.pause = Some(Pause::new());
        }
    }

    async fn complete_attempt(&mut self, result: PendingResult, recommit: bool) {
        if matches!(
            &result.result,
            ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::SharedDependency,
                _
            )
        ) {
            self.pause(&"shared dependency failure");
        }
        self.pending = Some(result);
        match self.finalise().await {
            Ok(FinalisationHealth::NoFreshFailure) if recommit => match self.probe().await {
                Ok(()) if self.pending.is_none() => {
                    self.pause = None;
                    tracing::info!("ingestion coordinator resumed");
                }
                Ok(()) => {}
                Err(error) => self.pause(&error),
            },
            Ok(FinalisationHealth::SharedFailure) => self.pause(&"fresh shared dependency failure"),
            Ok(FinalisationHealth::NoFreshFailure) => {}
            Err(error) => self.pause(&error),
        }
    }

    async fn probe(&mut self) -> anyhow::Result<()> {
        use sqlx::Connection;
        if let Some(connection) = &mut self.lock
            && connection.ping().await.is_err()
        {
            self.lock = None;
        }
        if self.lock.is_none() {
            let mut connection = self.pool.acquire().await?;
            connection.close_on_drop();
            let held = sqlx::query_scalar!(
                "SELECT pg_try_advisory_lock($1) AS \"held!\"",
                SCAN_ADVISORY_LOCK_ID
            )
            .fetch_one(&mut *connection)
            .await?;
            anyhow::ensure!(held, "ingestion ownership unavailable");
            self.lock = Some(connection);
        }
        if self.library_id.is_none() {
            self.library_id =
                Some(crate::models::storage_library::default_library_id(&self.pool).await?);
        }
        let library_id = self
            .library_id
            .ok_or_else(|| anyhow::anyhow!("library authority unavailable"))?;
        let files = self.files.clone();
        tokio::task::spawn_blocking(move || probe_roots(&files, library_id)).await??;
        if !self.recovered {
            self.recover_publications(None).await?;
            crate::models::ingestion_input::reclaim(&self.pool).await?;
            self.recovered = true;
        }
        Ok(())
    }

    async fn recover_publications(
        &mut self,
        selected: Option<&[crate::models::ingestion_input::InputPath]>,
    ) -> anyhow::Result<()> {
        use crate::models::ingestion_input;
        let mut unresolved = if selected.is_some() {
            self.unresolved.clone()
        } else {
            std::collections::HashSet::new()
        };
        let mut after = None;
        loop {
            let page =
                ingestion_input::publication_page_selected(&self.pool, after, selected).await?;
            if page.is_empty() {
                break;
            }
            after = page.last().map(|record| record.job);
            for publication in page {
                tracing::debug!(job = %publication.job, class = ?publication.failure_class,
                    reason = publication.failure_reason.as_deref(), "reconciling ingestion publication");
                match recover_publication(
                    &self.pool,
                    &self.files,
                    &publication,
                    PublicationRecovery::Startup,
                )
                .await
                {
                    Ok(_) => {
                        unresolved.remove(&publication.input_id);
                    }
                    Err(error) => {
                        let class = finalisation_class(
                            &error,
                            &self.files,
                            LibraryId::from_uuid(publication.library_id),
                        )
                        .await;
                        ingestion_input::defer_publication(
                            &self.pool,
                            publication.job,
                            class,
                            &error.to_string(),
                        )
                        .await?;
                        self.unresolved.insert(publication.input_id);
                        if class == ingestion_input::AttemptOutcome::SharedDependency {
                            return Err(error);
                        }
                        tracing::warn!(input = %publication.input_id, %error, "ingestion publication remains unresolved");
                        unresolved.insert(publication.input_id);
                        self.forget(publication.input_id);
                    }
                }
            }
        }
        self.unresolved = unresolved;
        Ok(())
    }

    fn forget(&mut self, id: Uuid) {
        if let Some(key) = self.keys.remove(&id) {
            self.deadlines.remove(&key);
        }
        self.ready.retain(|queued| *queued != id);
        self.ready_set.remove(&id);
        self.inputs.remove(&id);
    }

    fn schedule(
        &mut self,
        input: crate::models::ingestion_input::Input,
        retry: &crate::models::ingestion_input::RetryState,
    ) -> usize {
        use crate::models::ingestion_input::InputStatus;
        let id = input.id;
        let unchanged = self.inputs.get(&id).is_some_and(|old| {
            old.generation == input.generation && old.retry_reset_at == input.retry_reset_at
        });
        if !unchanged {
            self.forget(id);
        }
        let eligible = !self.unresolved.contains(&id)
            && self
                .accepted
                .as_ref()
                .is_none_or(|formats| formats.iter().any(|format| format == "epub"))
            && match input.status {
                InputStatus::Pending => true,
                InputStatus::OperationalFailure => !retry.needs_change && retry.count < 6,
                _ => false,
            };
        if !eligible {
            tracing::debug!(input = %id, status = ?input.status, reason = input.reason.as_deref(), work = ?input.work_id, "ingestion generation suppressed");
            self.forget(id);
            self.inputs.insert(id, input);
            return 2;
        }
        let now = tokio::time::Instant::now();
        let elapsed = (chrono::Utc::now() - input.observed_at)
            .to_std()
            .unwrap_or_default()
            .min(READINESS);
        let observed = self
            .observed
            .get(&input.source_path)
            .map_or(now - elapsed, |(_, observed)| *observed);
        let mut deadline = observed + READINESS;
        if retry.count > 0
            && let Some(failed_at) = retry.failed_at
        {
            let index = usize::try_from(retry.count - 1).unwrap_or(RETRIES.len());
            if let Some(delay) = RETRIES.get(index) {
                let remaining = (failed_at
                    + chrono::Duration::seconds(i64::try_from(*delay).unwrap_or(i64::MAX))
                    - chrono::Utc::now())
                .to_std()
                .unwrap_or_default();
                deadline = deadline.max(now + remaining);
            }
        }
        self.inputs.insert(id, input);
        if self.keys.contains_key(&id) {
            return 1;
        }
        if self.ready_set.contains(&id) {
            return 0;
        }
        if deadline <= now {
            self.ready.push_back(id);
            self.ready_set.insert(id);
            0
        } else {
            self.keys.insert(id, self.deadlines.insert_at(id, deadline));
            1
        }
    }

    async fn discover(&mut self, reset: bool) -> anyhow::Result<DiscoveryResult> {
        self.discover_selected(reset, None, true).await
    }

    async fn observe_paths(&mut self, paths: Vec<PathBuf>) -> anyhow::Result<DiscoveryResult> {
        use crate::models::ingestion_input::InputPath;
        let mut selected = paths
            .into_iter()
            .filter_map(|path| {
                path.strip_prefix(self.config.ingestion_path.as_path())
                    .ok()
                    .map(Path::to_owned)
            })
            .filter(|path| {
                !path.components().any(|part| {
                    part.as_os_str().as_encoded_bytes().starts_with(b".")
                        || part.as_os_str() == "Thumbs.db"
                })
            })
            .collect::<Vec<_>>();
        selected.sort();
        selected.dedup();
        if selected.iter().any(|path| path.as_os_str().is_empty()) {
            return self.discover_selected(false, None, true).await;
        }
        let mut compact = Vec::<PathBuf>::new();
        for path in selected {
            if !compact.iter().any(|parent| path.starts_with(parent)) {
                compact.push(path);
            }
        }
        let selected = compact
            .iter()
            .map(|path| InputPath::from_path(path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = DiscoveryResult {
            queued: 0,
            deferred: 0,
            suppressed: 0,
            monitor: "/api/v1/dashboard/activity",
        };
        for paths in selected.chunks(100) {
            if !self.unresolved.is_empty() {
                self.recover_publications(Some(paths)).await?;
            }
            let observed = self
                .discover_selected(false, Some(paths.to_vec()), true)
                .await?;
            result.queued += observed.queued;
            result.deferred += observed.deferred;
            result.suppressed += observed.suppressed;
        }
        Ok(result)
    }

    async fn refresh_observations(
        &mut self,
        selected: Option<&[crate::models::ingestion_input::InputPath]>,
    ) -> anyhow::Result<bool> {
        use crate::models::ingestion_input;
        use std::os::unix::ffi::OsStrExt;
        let (observations, complete) = {
            let files = self.files.clone();
            let paths = selected.map(<[_]>::to_vec);
            tokio::task::spawn_blocking(move || discover_files(files.ingestion(), paths.as_deref()))
                .await??
        };
        if selected.is_none() && complete {
            self.discovered = true;
        }
        let now = tokio::time::Instant::now();
        for (path, fingerprint) in &observations {
            let bytes = path.path().as_os_str().as_bytes().to_vec();
            let entry = self
                .observed
                .entry(bytes)
                .or_insert_with(|| (fingerprint.clone(), now));
            if entry.0 != *fingerprint {
                *entry = (fingerprint.clone(), now);
            }
        }
        for chunk in observations.chunks(100) {
            let paths = chunk
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>();
            let fingerprints = chunk
                .iter()
                .map(|(_, fingerprint)| fingerprint.clone())
                .collect::<Vec<_>>();
            ingestion_input::observe(&self.pool, &paths, &fingerprints).await?;
        }
        Ok(complete)
    }

    async fn discover_selected(
        &mut self,
        reset: bool,
        selected: Option<Vec<crate::models::ingestion_input::InputPath>>,
        observe: bool,
    ) -> anyhow::Result<DiscoveryResult> {
        use crate::models::ingestion_input::{self, InputPath, InputStatus};
        if reset {
            self.recover_publications(None).await?;
        }
        let complete = if observe {
            self.refresh_observations(selected.as_deref()).await?
        } else {
            false
        };
        let now = tokio::time::Instant::now();
        if reset {
            ingestion_input::reset_retries(&self.pool).await?;
        }
        let accepted = self
            .settings
            .read()
            .await
            .ingestion
            .accepted_formats
            .clone();
        self.accepted = Some(accepted.clone());
        let mut result = DiscoveryResult {
            queued: 0,
            deferred: 0,
            suppressed: 0,
            monitor: "/api/v1/dashboard/activity",
        };
        let mut after = None;
        loop {
            let mut page = if let Some(paths) = &selected {
                ingestion_input::selected_page(&self.pool, paths, after).await?
            } else {
                ingestion_input::current_page(&self.pool, after).await?
            };
            if page.is_empty() {
                break;
            }
            after = page.last().map(|input| input.id);
            if complete {
                self.reconcile_page(&mut page, now).await?;
            }
            let mut unaccepted = Vec::new();
            let mut generations = Vec::new();
            let mut newly_accepted = Vec::new();
            let mut accepted_generations = Vec::new();
            for input in &mut page {
                let path = InputPath::from_bytes(input.source_path.clone())?.path();
                let supported = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
                    && accepted.iter().any(|format| format == "epub");
                if !supported && input.status == InputStatus::Pending {
                    unaccepted.push(input.id);
                    generations.push(input.generation);
                    input.status = InputStatus::NotAccepted;
                } else if supported && input.status == InputStatus::NotAccepted {
                    newly_accepted.push(input.id);
                    accepted_generations.push(input.generation);
                    input.status = InputStatus::Pending;
                }
            }
            ingestion_input::set_unaccepted_many(&self.pool, &unaccepted, &generations).await?;
            ingestion_input::set_accepted_many(&self.pool, &newly_accepted, &accepted_generations)
                .await?;
            let ids = page.iter().map(|input| input.id).collect::<Vec<_>>();
            let retry = ingestion_input::retry_states(&self.pool, &ids).await?;
            for input in page {
                if let Some(retry) = retry.iter().find(|retry| retry.id == input.id) {
                    match self.schedule(input, retry) {
                        0 => result.queued += 1,
                        1 => result.deferred += 1,
                        _ => result.suppressed += 1,
                    }
                }
            }
        }
        Ok(result)
    }

    async fn reconcile_page(
        &mut self,
        page: &mut Vec<crate::models::ingestion_input::Input>,
        now: tokio::time::Instant,
    ) -> anyhow::Result<()> {
        use crate::models::ingestion_input::{self, Fingerprint, InputPath};
        let phase_files = self.files.clone();
        let check = page
            .iter()
            .filter(|input| {
                !self.pending.as_ref().is_some_and(|pending| {
                    pending.source_deleted
                        && pending.input.id == input.id
                        && pending.input.generation == input.generation
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        let checked = tokio::task::spawn_blocking(move || {
            check
                .into_iter()
                .map(|input| {
                    let metadata = InputPath::from_bytes(input.source_path.clone())
                        .and_then(|path| copier::input_metadata(phase_files.ingestion(), &path));
                    (input, metadata)
                })
                .collect::<Vec<_>>()
        })
        .await?;
        let mut missing = Vec::new();
        let mut generations = Vec::new();
        let mut paths = Vec::new();
        let mut fingerprints = Vec::new();
        for (input, metadata) in checked {
            match metadata {
                Ok(metadata) => {
                    let fingerprint = Fingerprint::from_metadata(&metadata);
                    if input.fingerprint.0 != fingerprint {
                        let path = InputPath::from_bytes(input.source_path.clone())?;
                        self.observed
                            .insert(input.source_path, (fingerprint.clone(), now));
                        paths.push(path);
                        fingerprints.push(fingerprint);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(input.id);
                    generations.push(input.generation);
                }
                Err(error) => {
                    tracing::warn!(kind = ?error.kind(), "ingestion observation unavailable");
                }
            }
        }
        ingestion_input::remove_many(
            &self.pool,
            &missing,
            &generations,
            "unattributed_disappearance",
        )
        .await?;
        for input in page.iter().filter(|input| missing.contains(&input.id)) {
            self.observed.remove(&input.source_path);
            self.forget(input.id);
        }
        if !paths.is_empty() {
            let updates = ingestion_input::observe(&self.pool, &paths, &fingerprints).await?;
            for updated in updates {
                if let Some(input) = page.iter_mut().find(|input| input.id == updated.id) {
                    *input = updated;
                }
            }
        }
        page.retain(|input| !missing.contains(&input.id));
        Ok(())
    }

    async fn start_attempt(
        &mut self,
        cancel: &CancellationToken,
    ) -> anyhow::Result<Option<Active>> {
        if self.pending.is_none()
            && self.pause.is_none()
            && let Some(id) = self.ready.pop_front()
            && let Some(input) = self.inputs.get(&id).cloned()
            && let Some(library_id) = self.library_id
        {
            self.ready_set.remove(&id);
            if !self
                .settings
                .read()
                .await
                .ingestion
                .accepted_formats
                .iter()
                .any(|format| format == "epub")
            {
                if let Err(error) = self.discover_selected(false, None, false).await {
                    self.pause(&error);
                }
                return Ok(None);
            }
            match crate::models::ingestion_input::begin_attempt(&self.pool, &input, Uuid::new_v4())
                .await
            {
                Ok(job) => {
                    return self
                        .active_attempt(input, job, library_id, cancel)
                        .map(Some);
                }
                Err(error) => {
                    self.ready.push_front(id);
                    self.ready_set.insert(id);
                    self.pause(&error);
                }
            }
        }
        Ok(None)
    }

    fn active_attempt(
        &self,
        input: crate::models::ingestion_input::Input,
        job: Uuid,
        library_id: LibraryId,
        cancel: &CancellationToken,
    ) -> anyhow::Result<Active> {
        use futures::FutureExt;
        let config = self.config.clone();
        let pool = self.pool.clone();
        let files = self.files.clone();
        let progress = copier::Progress::new(cancel);
        let attempt_progress = progress.clone();
        let path = config.ingestion_path.as_path().join(
            crate::models::ingestion_input::InputPath::from_bytes(input.source_path.clone())?
                .path(),
        );
        let now = tokio::time::Instant::now();
        let future = async move {
            let mut result = std::panic::AssertUnwindSafe(process_file(
                &path,
                &config,
                &pool,
                &files,
                library_id,
                Some(&input),
                attempt_progress,
            ))
            .catch_unwind()
            .await
            .unwrap_or_else(|_| {
                ProcessResult::Operational(
                    crate::models::ingestion_input::AttemptOutcome::TransientInput,
                    "panic during ingestion attempt".into(),
                )
            });
            let mut health = if matches!(
                &result,
                ProcessResult::Operational(
                    crate::models::ingestion_input::AttemptOutcome::SharedDependency,
                    _
                )
            ) {
                FinalisationHealth::SharedFailure
            } else {
                FinalisationHealth::NoFreshFailure
            };
            if let ProcessResult::Accepted(accepted) = &mut result {
                match std::panic::AssertUnwindSafe(finish_accepted(
                    accepted,
                    &config,
                    &pool,
                    &files,
                    Some((&input, job)),
                ))
                .catch_unwind()
                .await
                {
                    Ok(Ok(completed)) => {
                        health = accepted.health;
                        result = completed;
                    }
                    Ok(Err(error)) => {
                        tracing::warn!(%error, "accepted result retained for recommit");
                    }
                    Err(_) => tracing::warn!("accepted result retained after finalisation panic"),
                }
            }
            PendingResult {
                input,
                job,
                result,
                source_deleted: false,
                health,
            }
        }
        .boxed();
        Ok(Active {
            future,
            progress,
            count: 0,
            transitions: 0,
            last_progress: now,
            next_warning: now + STALL,
            recommit: false,
        })
    }

    async fn reprobe(&mut self) -> anyhow::Result<Option<Active>> {
        if self
            .pause
            .as_ref()
            .is_some_and(|pause| tokio::time::Instant::now() >= pause.next)
        {
            match self.probe().await {
                Ok(()) => {
                    if let Some(pending) = &mut self.pending {
                        pending.health = FinalisationHealth::NoFreshFailure;
                    }
                    if self
                        .pending
                        .as_ref()
                        .is_some_and(|pending| matches!(pending.result, ProcessResult::Accepted(_)))
                    {
                        if let Some(pending) = self.pending.take() {
                            if let Some(pause) = &mut self.pause {
                                pause.failed();
                            }
                            return Ok(Some(Active::recommit(
                                pending,
                                self.config.clone(),
                                self.pool.clone(),
                                self.files.clone(),
                            )));
                        }
                        if let Some(pause) = &mut self.pause {
                            pause.failed();
                        }
                        return Ok(None);
                    }
                    let finalised = self.finalise().await;
                    if let Err(error) = &finalised {
                        if let Some(pause) = &mut self.pause {
                            pause.failed();
                        }
                        tracing::debug!(%error, "ingestion result recommit deferred");
                    } else if matches!(finalised, Ok(FinalisationHealth::NoFreshFailure)) {
                        self.pause = None;
                        tracing::info!("ingestion coordinator resumed");
                        if let Err(error) =
                            self.discover_selected(false, None, !self.discovered).await
                        {
                            self.pause(&error);
                        }
                    } else if let Some(pause) = &mut self.pause {
                        pause.failed();
                    }
                }
                Err(error) => {
                    if let Some(pause) = &mut self.pause {
                        pause.failed();
                    }
                    tracing::debug!(%error, "ingestion dependency probe failed");
                }
            }
        }
        Ok(None)
    }

    async fn commit_pending(&self, pending: &mut PendingResult) -> anyhow::Result<()> {
        use crate::models::ingestion_input::{self, AttemptOutcome, InputStatus};
        if let ProcessResult::Accepted(accepted) = &mut pending.result {
            let completed = finish_accepted(
                accepted,
                &self.config,
                &self.pool,
                &self.files,
                Some((&pending.input, pending.job)),
            )
            .await?;
            pending.health = accepted.health;
            pending.result = completed;
        }
        let (outcome, status, reason, work) = match &pending.result {
            ProcessResult::Complete => return Ok(()),
            ProcessResult::Skipped(work) => (
                AttemptOutcome::Duplicate,
                InputStatus::Duplicate,
                None,
                Some(*work),
            ),
            ProcessResult::Failed(reason) => (
                AttemptOutcome::Rejected,
                InputStatus::Rejected,
                Some(reason.as_str()),
                None,
            ),
            ProcessResult::Operational(class, reason) => (
                *class,
                if *class == AttemptOutcome::SharedDependency {
                    InputStatus::Pending
                } else {
                    InputStatus::OperationalFailure
                },
                Some(reason.as_str()),
                None,
            ),
            ProcessResult::Changed => (AttemptOutcome::Changed, InputStatus::Pending, None, None),
            ProcessResult::Accepted(_) => anyhow::bail!("accepted result awaiting commit"),
        };
        let mut tx = self.pool.begin().await?;
        ingestion_input::finish(
            &mut tx,
            &pending.input,
            pending.job,
            outcome,
            status,
            reason,
            work,
        )
        .await?;
        tx.commit().await?;
        Ok::<(), anyhow::Error>(())
    }

    async fn finalise(&mut self) -> anyhow::Result<FinalisationHealth> {
        use crate::models::ingestion_input::{self, AttemptOutcome};
        if self.pending.is_some() && self.library_id.is_none() {
            self.library_id =
                Some(crate::models::storage_library::default_library_id(&self.pool).await?);
        }
        let Some(mut pending) = self.pending.take() else {
            return Ok(FinalisationHealth::NoFreshFailure);
        };
        let result = self.commit_pending(&mut pending).await;
        match result {
            Ok(()) => {
                if let Err(error) = self.cleanup(&mut pending).await {
                    let library_id = self
                        .library_id
                        .ok_or_else(|| anyhow::anyhow!("library authority unavailable"))?;
                    if pending.source_deleted
                        || finalisation_class(&error, &self.files, library_id).await
                            == AttemptOutcome::SharedDependency
                    {
                        self.pending = Some(pending);
                        return Err(error);
                    }
                    tracing::warn!(input = %pending.input.id, %error, "completed ingestion source retained after cleanup failure");
                }
                self.forget(pending.input.id);
                let path =
                    ingestion_input::InputPath::from_bytes(pending.input.source_path.clone())?;
                self.discover_selected(false, Some(vec![path]), true)
                    .await?;
                Ok(pending.health)
            }
            Err(error) => {
                let library_id = self
                    .library_id
                    .ok_or_else(|| anyhow::anyhow!("library authority unavailable"))?;
                if matches!(pending.result, ProcessResult::Accepted(_))
                    && finalisation_class(&error, &self.files, library_id).await
                        != AttemptOutcome::SharedDependency
                {
                    self.unresolved.insert(pending.input.id);
                    self.forget(pending.input.id);
                    tracing::warn!(input = %pending.input.id, %error, "ingestion input suspended with publication evidence");
                    return Ok(FinalisationHealth::NoFreshFailure);
                }
                self.pending = Some(pending);
                Err(error)
            }
        }
    }

    async fn cleanup(&self, pending: &mut PendingResult) -> anyhow::Result<()> {
        use crate::models::ingestion_input::{self, InputPath, InputStatus};
        let Some(current) = ingestion_input::current(&self.pool, pending.input.id).await? else {
            return Ok(());
        };
        if current.generation != pending.input.generation {
            return Ok(());
        }
        if pending.source_deleted {
            ingestion_input::remove(&self.pool, &current, "automatic_cleanup").await?;
            return Ok(());
        }
        let settings = self.settings.read().await;
        let enabled = match current.status {
            InputStatus::Imported => settings.ingestion.cleanup_imported,
            InputStatus::Duplicate => settings.ingestion.cleanup_duplicates,
            _ => false,
        };
        drop(settings);
        if !enabled {
            return Ok(());
        }
        let files = self.files.clone();
        let path = InputPath::from_bytes(current.source_path.clone())?;
        let fingerprint = current.fingerprint.0.clone();
        pending.source_deleted = tokio::task::spawn_blocking(move || {
            cleanup::remove_verified(files.ingestion(), &path, &fingerprint)
        })
        .await??;
        if pending.source_deleted {
            ingestion_input::remove(&self.pool, &current, "automatic_cleanup").await?;
        }
        Ok(())
    }
}

fn probe_roots(files: &LibraryFiles, library_id: LibraryId) -> std::io::Result<()> {
    use std::io::Write;
    files.ingestion().dir_metadata()?;
    let _ = files.ingestion().entries()?.next().transpose()?;
    let root = files.library(library_id).map_err(std::io::Error::other)?;
    root.dir_metadata()?;
    let mut probe = cap_tempfile::TempFile::new(root)?;
    probe.write_all(&[0])?;
    probe.as_file().sync_all()
}

fn discover_files(
    root: &cap_std::fs::Dir,
    selected: Option<&[crate::models::ingestion_input::InputPath]>,
) -> std::io::Result<(
    Vec<(
        crate::models::ingestion_input::InputPath,
        crate::models::ingestion_input::Fingerprint,
    )>,
    bool,
)> {
    use crate::models::ingestion_input::{Fingerprint, InputPath};
    use std::os::unix::ffi::OsStrExt;
    let mut todo = Vec::new();
    let mut files = Vec::new();
    let mut complete = true;
    if let Some(selected) = selected {
        for path in selected {
            match copier::input_metadata(root, path) {
                Ok(metadata) if metadata.is_dir() => todo.push(path.path()),
                Ok(metadata) if metadata.is_file() => {
                    files.push((path.clone(), Fingerprint::from_metadata(&metadata)));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    complete = false;
                    tracing::warn!(kind = ?error.kind(), "ingestion observation unavailable");
                }
            }
        }
    } else {
        todo.push(PathBuf::new());
    }
    while let Some(path) = todo.pop() {
        let directory = if path.as_os_str().is_empty() {
            root.try_clone()
        } else {
            let child = InputPath::from_path(&path.join("entry"))?;
            copier::source_parent(root, &child).map(|(parent, _)| parent)
        };
        let directory = match directory {
            Ok(directory) => directory,
            Err(error) => {
                complete = false;
                tracing::warn!(kind = ?error.kind(), "ingestion directory unreadable");
                continue;
            }
        };
        let entries = match directory.entries() {
            Ok(entries) => entries,
            Err(error) => {
                complete = false;
                tracing::warn!(kind = ?error.kind(), "ingestion directory unreadable");
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            let name = entry.file_name();
            if name.as_bytes().starts_with(b".") || name == "Thumbs.db" {
                continue;
            }
            let relative = path.join(name);
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => todo.push(relative),
                Ok(kind) if kind.is_file() => {
                    let input_path = InputPath::from_path(&relative)?;
                    match copier::input_metadata(root, &input_path) {
                        Ok(metadata) => {
                            files.push((input_path, Fingerprint::from_metadata(&metadata)));
                        }
                        Err(_) => complete = false,
                    }
                }
                Ok(_) => {}
                Err(_) => complete = false,
            }
        }
    }
    Ok((files, complete))
}

/// Run discovery, readiness, attempts and recovery under one scheduling owner.
///
/// # Errors
/// Returns an error when a required input location cannot be represented.
pub async fn run_watcher(
    config: Config,
    pool: PgPool,
    cancel: CancellationToken,
    files: LibraryFiles,
    settings: std::sync::Arc<tokio::sync::RwLock<crate::models::settings::Settings>>,
    mut commands: CoordinatorCommands,
) -> Result<(), anyhow::Error> {
    use futures::StreamExt;
    let (tx, mut rx) = mpsc::channel::<Vec<PathBuf>>(16);
    let watcher_cancel = cancel.clone();
    let ingestion_path = PathBuf::from(&config.ingestion_path);
    tokio::spawn(async move {
        if let Err(error) = super::watcher::watch(ingestion_path, tx, watcher_cancel).await {
            tracing::error!(%error, "filesystem watcher failed");
        }
    });
    let mut owner = Coordinator {
        config,
        pool,
        files,
        settings,
        inputs: std::collections::HashMap::default(),
        deadlines: tokio_util::time::DelayQueue::default(),
        keys: std::collections::HashMap::default(),
        ready: std::collections::VecDeque::default(),
        ready_set: std::collections::HashSet::default(),
        observed: std::collections::HashMap::default(),
        pause: None,
        pending: None,
        lock: None,
        library_id: None,
        recovered: false,
        discovered: false,
        accepted: None,
        unresolved: std::collections::HashSet::default(),
    };
    if let Err(error) = owner.probe().await {
        owner.pause(&error);
    }
    if let Err(error) = owner.discover(false).await {
        owner.pause(&error);
    }
    let mut active: Option<Active> = None;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        if active.is_none() {
            active = owner.start_attempt(&cancel).await?;
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => {
                if let Some(mut active) = active.take() {
                    loop {
                        tokio::select! {
                            _ = &mut active.future => break,
                            _ = tick.tick() => active.tick(),
                        }
                    }
                }
                break;
            }
            result = async { match &mut active { Some(active) => (&mut active.future).await, None => std::future::pending().await } } => {
                let recommit = active.as_ref().is_some_and(|active| active.recommit);
                active = None;
                owner.complete_attempt(result, recommit).await;
            }
            _ = tick.tick() => {
                if let Some(active) = &mut active { active.tick(); }
                let accepted = owner.settings.read().await.ingestion.accepted_formats.clone();
                if owner.accepted.as_ref().is_some_and(|previous| *previous != accepted)
                    && let Err(error) = owner.discover_selected(false, None, false).await { owner.pause(&error); }
                if active.is_none() {
                    active = owner.reprobe().await?;
                }
            }
            expired = owner.deadlines.next(), if !owner.deadlines.is_empty() => {
                if let Some(expired) = expired {
                    let id = expired.into_inner();
                    owner.keys.remove(&id);
                    if owner.inputs.contains_key(&id) && owner.ready_set.insert(id) { owner.ready.push_back(id); }
                }
            }
            Some(reply) = commands.recv() => {
                let result = owner.discover(true).await;
                if let Err(error) = &result { owner.pause(error); }
                drop(reply.send(result));
            }
            Some(paths) = rx.recv() => {
                if let Err(error) = owner.observe_paths(paths).await { owner.pause(&error); }
            }
        }
    }
    Ok(())
}

/// Session ownership for ingestion discovery, attempts and recovery.
const SCAN_ADVISORY_LOCK_ID: i64 = 0x5265_7665_0000_0004;

enum ProcessResult {
    Complete,
    Accepted(Box<Accepted>),
    Skipped(Uuid),
    Failed(String),
    Operational(crate::models::ingestion_input::AttemptOutcome, String),
    Changed,
}

struct Accepted {
    candidate: std::sync::Arc<copier::Candidate>,
    source: crate::models::ingestion_input::InputPath,
    library_id: LibraryId,
    path: RelativeFilePath,
    vars: std::collections::HashMap<String, String>,
    extracted: Option<metadata::extractor::ExtractedMetadata>,
    validation_status: ValidationStatus,
    accessibility_metadata: Option<serde_json::Value>,
    has_embedded_cover: Option<bool>,
    current_hash: String,
    current_size: u64,
    published: Option<(LibraryLocation, copier::CopyResult)>,
    failure: Option<(crate::models::ingestion_input::AttemptOutcome, String)>,
    health: FinalisationHealth,
    recovering: bool,
    force_copy: bool,
    created_directories: Vec<path_rename::CreatedDirectory>,
}

#[derive(Debug, Eq, PartialEq)]
enum FailureDisposition {
    Changed,
    Failure(crate::models::ingestion_input::AttemptOutcome, String),
}

fn classify_acquisition_error(
    error: &copier::CopyError,
    source_changed: bool,
    roots_healthy: bool,
    source_operation: bool,
) -> FailureDisposition {
    use crate::models::ingestion_input::AttemptOutcome;
    if matches!(error, copier::CopyError::Changed)
        || (matches!(error, copier::CopyError::HashMismatch { .. }) && source_changed)
    {
        return FailureDisposition::Changed;
    }
    let io = match error {
        copier::CopyError::Io(error)
        | copier::CopyError::DestinationIo(error)
        | copier::CopyError::Persist(crate::services::writeback::error::WritebackError::Io(
            error,
        ))
        | copier::CopyError::Publication {
            error: crate::services::writeback::error::WritebackError::Io(error),
            ..
        } => Some(error),
        _ => None,
    };
    let destination = !source_operation
        || matches!(
            error,
            copier::CopyError::DestinationIo(_) | copier::CopyError::Publication { .. }
        );
    let class = if !roots_healthy || (destination && io.is_some_and(|error| error.raw_os_error() == Some(rustix::io::Errno::NOSPC.raw_os_error()))) {
        AttemptOutcome::SharedDependency
    } else if matches!(error, copier::CopyError::NonRegular)
        || io.is_some_and(|error| {
            matches!(error.raw_os_error(), Some(code) if code == rustix::io::Errno::NAMETOOLONG.raw_os_error() || code == rustix::io::Errno::LOOP.raw_os_error())
                || (!destination && matches!(error.raw_os_error(), Some(code) if code == rustix::io::Errno::ACCESS.raw_os_error() || code == rustix::io::Errno::PERM.raw_os_error()))
        }) {
        AttemptOutcome::NeedsChange
    } else {
        AttemptOutcome::TransientInput
    };
    FailureDisposition::Failure(class, copy_error_reason(error))
}

async fn acquisition_failure(
    error: copier::CopyError,
    files: &LibraryFiles,
    library_id: LibraryId,
    source_operation: bool,
) -> ProcessResult {
    let files = files.clone();
    match tokio::task::spawn_blocking(move || {
        let roots_healthy = probe_roots(&files, library_id).is_ok();
        classify_acquisition_error(&error, false, roots_healthy, source_operation)
    })
    .await
    {
        Ok(FailureDisposition::Changed) => ProcessResult::Changed,
        Ok(FailureDisposition::Failure(class, reason)) => ProcessResult::Operational(class, reason),
        Err(_) => ProcessResult::Operational(
            crate::models::ingestion_input::AttemptOutcome::TransientInput,
            "panic during error classification".into(),
        ),
    }
}

async fn select_ingestion_path(
    pool: &PgPool,
    files: &LibraryFiles,
    library_id: LibraryId,
    candidate: &RelativeFilePath,
) -> anyhow::Result<(LibraryLocation, sqlx::Transaction<'static, sqlx::Postgres>)> {
    let mut tx = pool.begin().await?;
    for suffix in 1..=999 {
        let path = path_template::collision_candidate(candidate, suffix)?;
        let location = LibraryLocation { library_id, path };
        library_path_claim::exclude(&mut tx, &location).await?;
        if library_path_claim::owner(&mut tx, &location)
            .await?
            .is_some()
        {
            continue;
        }
        let phase_files = files.clone();
        let phase_location = location.clone();
        let occupied = tokio::task::spawn_blocking(move || {
            match phase_files
                .library(library_id)?
                .symlink_metadata(phase_location.path.as_path())
            {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(crate::services::files::LibraryFileError::Io(error)),
            }
        })
        .await??;
        if !occupied {
            return Ok((location, tx));
        }
    }
    Err(FinalisationError::SuffixExhausted.into())
}

async fn cleanup_candidate(
    pool: &PgPool,
    files: &LibraryFiles,
    location: &LibraryLocation,
    identity: (u64, u64),
) -> anyhow::Result<()> {
    cleanup_candidate_with(pool, files, location, identity, |parent, name| {
        parent.remove_file(name)
    })
    .await
}

async fn cleanup_candidate_with(
    pool: &PgPool,
    files: &LibraryFiles,
    location: &LibraryLocation,
    identity: (u64, u64),
    remove: impl FnOnce(&cap_std::fs::Dir, &std::ffi::OsStr) -> std::io::Result<()> + Send + 'static,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    library_path_claim::exclude(&mut tx, location).await?;
    if library_path_claim::owner(&mut tx, location).await?.is_some()
        || sqlx::query_scalar!(
            "SELECT EXISTS (SELECT 1 FROM manifestations WHERE library_id = $1 AND file_path = $2) AS \"owned!\"",
            location.library_id.as_uuid(), location.path.as_str(),
        ).fetch_one(&mut *tx).await? {
        anyhow::bail!("library copy retained: committed or claimed ownership")
    }
    let files = files.clone();
    let location = location.clone();
    tokio::task::spawn_blocking(move || {
        use cap_std::fs::MetadataExt;

        // THREAT: Cancellation must not release exclusion while removal can still mutate the name.
        let _exclusion = tx;
        let (parent, name) =
            path_rename::parent(files.library(location.library_id)?, &location.path)?;
        let metadata = parent.symlink_metadata(&name)?;
        if !metadata.is_file() || (metadata.dev(), metadata.ino()) != identity {
            anyhow::bail!("library copy retained: candidate identity changed")
        }
        remove(&parent, &name)?;
        Ok(())
    })
    .await?
}

async fn discard_publication(
    pool: &PgPool,
    files: &LibraryFiles,
    location: &LibraryLocation,
    identity: (u64, u64),
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    library_path_claim::exclude(&mut tx, location).await?;
    if library_path_claim::owner(&mut tx, location)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let phase_files = files.clone();
    let phase_location = location.clone();
    let owned = tokio::task::spawn_blocking(move || {
        use cap_std::fs::MetadataExt;
        let (parent, name) = path_rename::parent(
            phase_files.library(phase_location.library_id)?,
            &phase_location.path,
        )?;
        match parent.symlink_metadata(name) {
            Ok(metadata) => Ok(metadata.is_file() && (metadata.dev(), metadata.ino()) == identity),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(anyhow::Error::from(error)),
        }
    })
    .await??;
    tx.rollback().await?;
    if owned {
        cleanup_candidate(pool, files, location, identity).await?;
    }
    Ok(())
}

async fn finalisation_class(
    error: &anyhow::Error,
    files: &LibraryFiles,
    library_id: LibraryId,
) -> crate::models::ingestion_input::AttemptOutcome {
    use crate::models::ingestion_input::AttemptOutcome;
    if error.downcast_ref::<FinalisationError>().is_some() {
        return AttemptOutcome::NeedsChange;
    }
    if let Some(error) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<sqlx::Error>())
    {
        return match error {
            sqlx::Error::Io(_)
            | sqlx::Error::Tls(_)
            | sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::WorkerCrashed => AttemptOutcome::SharedDependency,
            sqlx::Error::Database(error) => {
                let code = error.code();
                match code.as_deref() {
                    Some("40001" | "40P01") => AttemptOutcome::TransientInput,
                    Some(code)
                        if code.starts_with("08")
                            || code.starts_with("53")
                            || matches!(code, "57P01" | "57P02" | "57P03") =>
                    {
                        AttemptOutcome::SharedDependency
                    }
                    _ => AttemptOutcome::NeedsChange,
                }
            }
            _ => AttemptOutcome::NeedsChange,
        };
    }
    let io = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
        .or_else(
            || match error.downcast_ref::<crate::services::files::LibraryFileError>() {
                Some(crate::services::files::LibraryFileError::Io(error)) => Some(error),
                _ => None,
            },
        );
    let phase_files = files.clone();
    let healthy =
        match tokio::task::spawn_blocking(move || probe_roots(&phase_files, library_id)).await {
            Ok(Ok(())) => true,
            Ok(Err(error)) => {
                tracing::warn!(kind = ?error.kind(), "ingestion root probe failed");
                false
            }
            Err(error) => {
                tracing::warn!(%error, "ingestion root probe task failed");
                false
            }
        };
    if !healthy || io.is_some_and(|error| error.raw_os_error() == Some(rustix::io::Errno::NOSPC.raw_os_error())) {
        AttemptOutcome::SharedDependency
    } else if io.is_some_and(|error| matches!(error.raw_os_error(), Some(code)
        if code == rustix::io::Errno::NAMETOOLONG.raw_os_error() || code == rustix::io::Errno::LOOP.raw_os_error()))
        || error.downcast_ref::<crate::services::files::LibraryFileError>().is_some_and(|error|
            !matches!(error, crate::services::files::LibraryFileError::Io(_))) {
        AttemptOutcome::NeedsChange
    } else {
        AttemptOutcome::TransientInput
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationDisposition {
    Absent,
    Removed,
    Foreign,
    Registered,
    Verified,
}

#[derive(Debug, thiserror::Error)]
enum FinalisationError {
    #[error("ingestion collision suffix exhausted")]
    SuffixExhausted,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PublicationRecovery {
    Startup,
    Live,
    Inspect,
}

struct PublicationEvidence {
    identity: crate::models::ingestion_input::PublicationIdentity,
    hash: String,
    size: i64,
}

fn inspect_unregistered_publication(
    phase_files: &LibraryFiles,
    location: &LibraryLocation,
    publication: &PublicationEvidence,
    purpose: PublicationRecovery,
) -> anyhow::Result<PublicationDisposition> {
    use cap_std::fs::MetadataExt;
    let PublicationEvidence {
        identity,
        hash,
        size,
    } = publication;
    let root = phase_files.library(location.library_id)?;
    let (parent, name) = match path_rename::parent(root, &location.path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PublicationDisposition::Absent);
        }
        Err(error) => return Err(error.into()),
    };
    let metadata = match parent.symlink_metadata(&name) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            parent.open(".")?.sync_all()?;
            return Ok(PublicationDisposition::Absent);
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file()
        || (metadata.dev(), metadata.ino()) != (identity.device, identity.inode)
        || i128::from(metadata.len()) != i128::from(*size)
    {
        return Ok(PublicationDisposition::Foreign);
    }
    let fd = rustix::fs::openat(
        &parent,
        &name,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let mut file = std::fs::File::from(fd);
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata()?;
        if !opened.is_file()
            || (opened.dev(), opened.ino()) != (identity.device, identity.inode)
            || i128::from(opened.len()) != i128::from(*size)
        {
            return Ok(PublicationDisposition::Foreign);
        }
    }
    if crate::services::epub::repack::hash_file(&mut file)? != *hash {
        return Ok(PublicationDisposition::Foreign);
    }
    let current = parent.symlink_metadata(&name)?;
    if !current.is_file()
        || (current.dev(), current.ino()) != (identity.device, identity.inode)
        || i128::from(current.len()) != i128::from(*size)
    {
        return Ok(PublicationDisposition::Foreign);
    }
    if purpose == PublicationRecovery::Inspect {
        return Ok(PublicationDisposition::Verified);
    }
    parent.remove_file(&name)?;
    parent.open(".")?.sync_all()?;
    Ok::<_, anyhow::Error>(PublicationDisposition::Removed)
}

async fn recover_publication(
    pool: &PgPool,
    files: &LibraryFiles,
    publication: &crate::models::ingestion_input::Publication,
    purpose: PublicationRecovery,
) -> anyhow::Result<PublicationDisposition> {
    use crate::models::ingestion_input;
    let location = LibraryLocation {
        library_id: LibraryId::from_uuid(publication.library_id),
        path: publication.path.parse()?,
    };
    let mut tx = pool.begin().await?;
    library_path_claim::exclude(&mut tx, &location).await?;
    if publication.imported {
        ingestion_input::clear_publication(&mut tx, publication.job).await?;
        tx.commit().await?;
        return Ok(PublicationDisposition::Registered);
    }
    if library_path_claim::owner(&mut tx, &location)
        .await?
        .is_some()
    {
        if purpose == PublicationRecovery::Startup {
            ingestion_input::foreign_publication(&mut tx, publication).await?;
        } else {
            ingestion_input::clear_publication(&mut tx, publication.job).await?;
        }
        tx.commit().await?;
        return Ok(PublicationDisposition::Foreign);
    }
    let phase_files = files.clone();
    let evidence = PublicationEvidence {
        identity: publication.identity.0.clone(),
        hash: publication.hash.clone(),
        size: publication.size,
    };
    let (mut tx, disposition) = tokio::task::spawn_blocking(move || {
        // THREAT: Exclusion stays owned until evidence verification and possible deletion finish.
        let checked = inspect_unregistered_publication(&phase_files, &location, &evidence, purpose);
        (tx, checked)
    })
    .await?;
    let disposition = disposition?;
    if disposition == PublicationDisposition::Verified {
        tx.rollback().await?;
        return Ok(disposition);
    }
    if disposition == PublicationDisposition::Foreign && purpose == PublicationRecovery::Startup {
        ingestion_input::foreign_publication(&mut tx, publication).await?;
    } else {
        ingestion_input::clear_publication(&mut tx, publication.job).await?;
    }
    tx.commit().await?;
    Ok(disposition)
}

async fn process_file(
    source: &Path,
    config: &Config,
    pool: &PgPool,
    files: &LibraryFiles,
    library_id: LibraryId,
    input: Option<&crate::models::ingestion_input::Input>,
    progress: copier::Progress,
) -> ProcessResult {
    use crate::models::ingestion_input::{Fingerprint, InputPath};
    let Ok(path) = source
        .strip_prefix(config.ingestion_path.as_path())
        .map_err(std::io::Error::other)
        .and_then(InputPath::from_path)
    else {
        return ProcessResult::Operational(
            crate::models::ingestion_input::AttemptOutcome::NeedsChange,
            "unrepresentable source path".into(),
        );
    };
    let Some(filename) = source.file_name().and_then(|name| name.to_str()) else {
        return ProcessResult::Operational(
            crate::models::ingestion_input::AttemptOutcome::NeedsChange,
            "unrepresentable source path".into(),
        );
    };
    let vars = path_template::heuristic_vars_from_filename(filename);
    if vars
        .get("ext")
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("epub"))
    {
        return ProcessResult::Failed("unsupported format".into());
    }
    let rendered = path_template::render(path_template::DEFAULT_TEMPLATE, &vars);
    let candidate_path: RelativeFilePath = match checked_candidate(&rendered) {
        Ok(path) => path,
        Err(_) => {
            return ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::NeedsChange,
                "unrepresentable library path".into(),
            );
        }
    };
    let phase_files = files.clone();
    let phase_path = path.clone();
    let phase_candidate = candidate_path.clone();
    let expected = input.map(|input| input.fingerprint.0.clone());
    let acquired = tokio::task::spawn_blocking(move || {
        let source = copier::open_input(phase_files.ingestion(), &phase_path)?;
        let fingerprint = expected.unwrap_or(Fingerprint::from_metadata(&source.metadata()?));
        copier::acquire_controlled(
            phase_files.ingestion(),
            &phase_path,
            phase_files
                .library(library_id)
                .map_err(std::io::Error::other)?,
            &phase_candidate,
            &fingerprint,
            progress,
        )
    })
    .await;
    let candidate = match acquired {
        Ok(Ok(candidate)) => candidate,
        Ok(Err(error)) => return acquisition_failure(error, files, library_id, true).await,
        Err(_) => {
            return ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::TransientInput,
                "panic during acquisition".into(),
            );
        }
    };
    let duplicate = sqlx::query_scalar!(
        "SELECT work_id FROM manifestations WHERE ingestion_file_hash = $1 LIMIT 1",
        &candidate.ingestion_hash
    )
    .fetch_optional(pool)
    .await;
    match duplicate {
        Ok(Some(work)) => {
            if let Err(error) = candidate.close() {
                return acquisition_failure(
                    copier::CopyError::DestinationIo(error),
                    files,
                    library_id,
                    false,
                )
                .await;
            }
            return ProcessResult::Skipped(work);
        }
        Ok(None) => {}
        Err(error) => {
            return ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::SharedDependency,
                format!("duplicate confirmation failed: {error}"),
            );
        }
    }
    validate_candidate(candidate, path, candidate_path, vars, files, library_id).await
}

async fn validate_candidate(
    candidate: copier::Candidate,
    path: crate::models::ingestion_input::InputPath,
    candidate_path: RelativeFilePath,
    vars: std::collections::HashMap<String, String>,
    files: &LibraryFiles,
    library_id: LibraryId,
) -> ProcessResult {
    let forced_error = candidate_path.as_str().contains("force-validator-error");
    let validated = tokio::task::spawn_blocking(move || {
        candidate.progress().enter(copier::Phase::Validation)?;
        #[cfg(test)]
        let validation = if forced_error {
            Err(epub::EpubError::Io(std::io::Error::other(
                "forced validator error (test seam)",
            )))
        } else {
            candidate.validate()
        };
        #[cfg(not(test))]
        let validation = {
            let _ = forced_error;
            candidate.validate()
        };
        candidate.progress().check()?;
        let (hash, size) = candidate.accepted_bytes(&validation)?;
        Ok::<_, copier::CopyError>((candidate, validation, hash, size))
    })
    .await;
    let (candidate, validation, current_hash, current_size) = match validated {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => return acquisition_failure(error, files, library_id, false).await,
        Err(_) => {
            return ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::TransientInput,
                "panic during validation".into(),
            );
        }
    };
    accepted_from_validation(
        candidate,
        path,
        vars,
        library_id,
        files,
        (validation, current_hash, current_size),
    )
    .await
}

async fn accepted_from_validation(
    candidate: copier::Candidate,
    path: crate::models::ingestion_input::InputPath,
    vars: std::collections::HashMap<String, String>,
    library_id: LibraryId,
    files: &LibraryFiles,
    validated: (
        Result<crate::services::epub::Validated, crate::services::epub::EpubError>,
        String,
        u64,
    ),
) -> ProcessResult {
    let (validation, current_hash, current_size) = validated;
    let (validation_status, accessibility_metadata, opf_data, has_embedded_cover) = match validation
    {
        Ok(validated) => {
            let report = validated.report;
            if report.outcome == ValidationOutcome::Quarantined {
                let reason = report
                    .issues
                    .iter()
                    .map(|issue| format!("{:?}", issue.kind))
                    .collect::<Vec<_>>()
                    .join("; ");
                if let Err(error) = candidate.close() {
                    return acquisition_failure(
                        copier::CopyError::DestinationIo(error),
                        files,
                        library_id,
                        false,
                    )
                    .await;
                }
                return ProcessResult::Failed(format!("EPUB rejected: {reason}"));
            }
            let status = match report.outcome {
                ValidationOutcome::Clean => ValidationStatus::Clean,
                ValidationOutcome::Repaired => ValidationStatus::Repaired,
                ValidationOutcome::Degraded => ValidationStatus::Degraded,
                ValidationOutcome::Quarantined => unreachable!(),
            };
            (
                status,
                report.accessibility_metadata,
                report.opf_data,
                Some(report.has_usable_embedded_cover),
            )
        }
        Err(error) => {
            tracing::warn!(%error, "EPUB validator execution failed");
            (ValidationStatus::Failed, None, None, None)
        }
    };
    let extracted = opf_data.as_ref().map(metadata::extractor::extract);
    let mut final_vars = vars.clone();
    if let Some(meta) = extracted.as_ref() {
        if let Some(title) = meta.title.as_ref() {
            final_vars.insert("Title".into(), title.clone());
        }
        if let Some(author) = meta.first_author() {
            final_vars.insert("Author".into(), author.sort_name.clone());
        }
    }
    let rendered = path_template::render(path_template::DEFAULT_TEMPLATE, &final_vars);
    let final_candidate: RelativeFilePath = match checked_candidate(&rendered) {
        Ok(path) => path,
        Err(_) => {
            return ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::NeedsChange,
                "unrepresentable library path".into(),
            );
        }
    };
    ProcessResult::Accepted(Box::new(Accepted {
        candidate: std::sync::Arc::new(candidate),
        source: path,
        library_id,
        path: final_candidate,
        vars,
        extracted,
        validation_status,
        accessibility_metadata,
        has_embedded_cover,
        current_hash,
        current_size,
        published: None,
        failure: None,
        health: FinalisationHealth::NoFreshFailure,
        recovering: false,
        force_copy: false,
        created_directories: Vec::new(),
    }))
}

async fn finish_accepted(
    accepted: &mut Accepted,
    config: &Config,
    pool: &PgPool,
    files: &LibraryFiles,
    attempt: Option<(&crate::models::ingestion_input::Input, Uuid)>,
) -> anyhow::Result<ProcessResult> {
    use crate::models::ingestion_input;
    accepted.health = FinalisationHealth::NoFreshFailure;
    accepted.recovering = accepted.published.is_some();
    if let Some((_, job)) = attempt {
        match ingestion_input::imported_attempt(pool, job).await {
            Ok(true) => return Ok(ProcessResult::Complete),
            Ok(false) => {}
            Err(error) => {
                let error = anyhow::Error::from(error);
                let class = finalisation_class(&error, files, accepted.library_id).await;
                if class == ingestion_input::AttemptOutcome::SharedDependency {
                    return Err(error);
                }
                return Ok(ProcessResult::Operational(class, error.to_string()));
            }
        }
    }
    match finish_accepted_inner(accepted, config, pool, files, attempt).await {
        Ok(result) => {
            if accepted.failure.is_none()
                && matches!(
                    &result,
                    ProcessResult::Operational(
                        ingestion_input::AttemptOutcome::SharedDependency,
                        _
                    )
                )
            {
                accepted.health = FinalisationHealth::SharedFailure;
            }
            Ok(result)
        }
        Err(error) => {
            if let Some((_, job)) = attempt
                && ingestion_input::imported_attempt(pool, job).await?
            {
                return Ok(ProcessResult::Complete);
            }
            let class = finalisation_class(&error, files, accepted.library_id).await;
            if class == ingestion_input::AttemptOutcome::SharedDependency {
                accepted.health = FinalisationHealth::SharedFailure;
            }
            if accepted.published.is_some() {
                accepted.failure = Some((class, error.to_string()));
                if let Some((_, job)) = attempt {
                    ingestion_input::defer_publication(pool, job, class, &error.to_string())
                        .await?;
                }
                dispose_accepted_publication(accepted, pool, files, attempt).await?;
            }
            if let Err(cleanup) = prune_library_parents(accepted, pool).await {
                tracing::warn!(%cleanup, "ingestion created directories retained after failure");
            }
            if error.chain().any(|cause| {
                matches!(
                    cause.downcast_ref::<copier::CopyError>(),
                    Some(copier::CopyError::Changed)
                )
            }) {
                return Ok(ProcessResult::Changed);
            }
            let class = accepted.failure.as_ref().map_or(class, |(class, _)| *class);
            Ok(ProcessResult::Operational(class, error.to_string()))
        }
    }
}

async fn dispose_accepted_publication(
    accepted: &mut Accepted,
    pool: &PgPool,
    files: &LibraryFiles,
    attempt: Option<(&crate::models::ingestion_input::Input, Uuid)>,
) -> anyhow::Result<()> {
    use crate::models::ingestion_input::{Publication, PublicationIdentity};
    let Some((location, copied)) = &accepted.published else {
        if let Err(error) = prune_library_parents(accepted, pool).await {
            tracing::warn!(%error, "ingestion created directories retained after disposal");
        }
        return Ok(());
    };
    if let Some((input, job)) = attempt {
        if crate::models::ingestion_input::imported_attempt(pool, job).await? {
            accepted.published = None;
            return Ok(());
        }
        let publication = Publication {
            job,
            input_id: input.id,
            input_generation: input.generation,
            library_id: location.library_id.as_uuid(),
            path: location.path.as_str().into(),
            identity: sqlx::types::Json(PublicationIdentity {
                device: copied.identity.0,
                inode: copied.identity.1,
            }),
            hash: accepted.current_hash.clone(),
            size: i64::try_from(accepted.current_size)?,
            failure_class: accepted.failure.as_ref().map(|(class, _)| *class),
            failure_reason: accepted.failure.as_ref().map(|(_, reason)| reason.clone()),
            imported: false,
        };
        if recover_publication(pool, files, &publication, PublicationRecovery::Live).await?
            == PublicationDisposition::Foreign
        {
            accepted.failure = Some((
                crate::models::ingestion_input::AttemptOutcome::NeedsChange,
                "publication name has another owner or changed content".into(),
            ));
        }
    } else {
        discard_publication(pool, files, location, copied.identity).await?;
    }
    accepted.published = None;
    if let Err(error) = prune_library_parents(accepted, pool).await {
        tracing::warn!(%error, "ingestion created directories retained after publication disposal");
    }
    Ok(())
}

async fn prune_library_parents(accepted: &mut Accepted, pool: &PgPool) -> anyhow::Result<()> {
    if accepted.created_directories.is_empty() {
        return Ok(());
    }
    let paths = accepted
        .created_directories
        .iter()
        .map(|directory| directory.path.as_str().to_owned())
        .collect::<Vec<_>>();
    let ownership = sqlx::query!(
        "SELECT NOT row_security_active('public.library_path_claims') OR pg_has_role('reverie_ingestion', 'USAGE')
           OR (pg_has_role('reverie_app', 'USAGE') AND COALESCE(current_setting('app.system_context', TRUE) = 'writeback', FALSE)) AS \"available!\",
           ARRAY(SELECT path FROM UNNEST($2::text[]) AS directories(path)
             WHERE EXISTS (SELECT 1 FROM library_path_claims c WHERE c.library_id = $1 AND starts_with(c.path, directories.path || '/'))) AS \"owned!\"",
        accepted.library_id.as_uuid(), &paths,
    ).fetch_one(pool).await?;
    if !ownership.available {
        anyhow::bail!("directory ownership evidence unavailable")
    }
    let created = std::mem::take(&mut accepted.created_directories);
    tokio::task::spawn_blocking(move || {
        use cap_std::fs::MetadataExt;
        for directory in created.into_iter().rev() {
            if ownership
                .owned
                .contains(&directory.path.as_str().to_owned())
            {
                continue;
            }
            let metadata = match directory.parent.symlink_metadata(&directory.name) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if !metadata.is_dir()
                || (metadata.dev(), metadata.ino()) != directory.identity
                || metadata.dev() != directory.parent.dir_metadata()?.dev()
            {
                continue;
            }
            match directory.parent.remove_dir(&directory.name) {
                Ok(()) => directory.parent.open(".")?.sync_all()?,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        Ok::<_, std::io::Error>(())
    })
    .await??;
    Ok(())
}

async fn finish_accepted_inner(
    accepted: &mut Accepted,
    config: &Config,
    pool: &PgPool,
    files: &LibraryFiles,
    attempt: Option<(&crate::models::ingestion_input::Input, Uuid)>,
) -> anyhow::Result<ProcessResult> {
    if let Some((class, reason)) = &accepted.failure {
        let (class, reason) = (*class, reason.clone());
        dispose_accepted_publication(accepted, pool, files, attempt).await?;
        return Ok(ProcessResult::Operational(
            accepted.failure.as_ref().map_or(class, |(class, _)| *class),
            reason,
        ));
    }
    if accepted.candidate.progress().cancel.is_cancelled() {
        dispose_accepted_publication(accepted, pool, files, attempt).await?;
        return Ok(acquisition_failure(
            copier::CopyError::Cancelled,
            files,
            accepted.library_id,
            true,
        )
        .await);
    }
    if attempt.is_none()
        && let Some((location, copied)) = &accepted.published
    {
        let committed = sqlx::query!(
            "SELECT id, work_id FROM manifestations WHERE library_id = $1 AND file_path = $2 AND ingestion_file_hash = $3",
            location.library_id.as_uuid(), location.path.as_str(), &copied.sha256,
        ).fetch_optional(pool).await?;
        if let Some(row) = committed {
            warm_accepted(accepted, config, files, row.id).await;
            return Ok(ProcessResult::Complete);
        }
    }
    if accepted.published.is_none()
        && let Some(result) = publish_accepted(accepted, pool, files, attempt).await?
    {
        return Ok(result);
    }
    commit_accepted(accepted, config, pool, files, attempt).await
}

async fn prepare_accepted_publication(
    accepted: &mut Accepted,
    pool: &PgPool,
    files: &LibraryFiles,
    attempt: Option<(&crate::models::ingestion_input::Input, Uuid)>,
) -> anyhow::Result<(LibraryLocation, copier::Prepared)> {
    let (location, mut publication) =
        select_ingestion_path(pool, files, accepted.library_id, &accepted.path).await?;
    let phase_files = files.clone();
    let phase_location = location.clone();
    let candidate = accepted.candidate.clone();
    let hash = accepted.current_hash.clone();
    let size = accepted.current_size;
    let force_copy = accepted.force_copy;
    let (created, prepared) = tokio::task::spawn_blocking(move || {
        let mut created = Vec::new();
        let result = (|| {
            candidate.progress().check()?;
            let root = phase_files
                .library(phase_location.library_id)
                .map_err(std::io::Error::other)?;
            path_rename::prepare_destination_tracked(root, &phase_location.path, &mut created)?;
            candidate.prepare(root, &phase_location.path, &hash, size, force_copy)
        })();
        (created, result)
    })
    .await?;
    accepted.created_directories.extend(created);
    let prepared = prepared?;
    accepted
        .candidate
        .progress()
        .enter(copier::Phase::Publication)?;
    accepted.published = Some((location.clone(), prepared.copied.clone()));
    if let Some((input, job)) = attempt {
        use crate::models::ingestion_input::{self, PublicationIdentity};
        ingestion_input::record_publication(
            &mut publication,
            input,
            job,
            &location,
            &PublicationIdentity {
                device: prepared.copied.identity.0,
                inode: prepared.copied.identity.1,
            },
            &accepted.current_hash,
            accepted.current_size,
        )
        .await?;
    }
    publication.commit().await?;
    Ok((location, prepared))
}

async fn publish_accepted(
    accepted: &mut Accepted,
    pool: &PgPool,
    files: &LibraryFiles,
    attempt: Option<(&crate::models::ingestion_input::Input, Uuid)>,
) -> anyhow::Result<Option<ProcessResult>> {
    for _ in 0..999 {
        let (location, prepared) =
            prepare_accepted_publication(accepted, pool, files, attempt).await?;
        let mut publication = pool.begin().await?;
        library_path_claim::exclude(&mut publication, &location).await?;
        let claimed = library_path_claim::owner(&mut publication, &location)
            .await?
            .is_some();
        let phase_files = files.clone();
        let phase_location = location.clone();
        let occupied = tokio::task::spawn_blocking(move || {
            let (parent, name) = path_rename::parent(
                phase_files.library(phase_location.library_id)?,
                &phase_location.path,
            )?;
            match parent.symlink_metadata(name) {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(anyhow::Error::from(error)),
            }
        })
        .await??;
        if claimed || occupied {
            publication.rollback().await?;
            dispose_accepted_publication(accepted, pool, files, attempt).await?;
            accepted.failure = None;
            continue;
        }
        let phase_files = files.clone();
        let phase_path = accepted.source.clone();
        let phase_location = location.clone();
        let candidate = accepted.candidate.clone();
        let published = tokio::task::spawn_blocking(move || {
            let _publication = publication;
            candidate.progress().check()?;
            candidate.verify_source(phase_files.ingestion(), &phase_path)?;
            let result = prepared.publish(
                phase_files
                    .library(phase_location.library_id)
                    .map_err(std::io::Error::other)?,
                &phase_location.path,
            );
            if matches!(result, Err(copier::CopyError::HashMismatch { .. })) {
                candidate.verify_source(phase_files.ingestion(), &phase_path)?;
            }
            result
        })
        .await;
        let published = match published {
            Ok(result) => result,
            Err(error) => {
                accepted.failure = Some((
                    crate::models::ingestion_input::AttemptOutcome::TransientInput,
                    "panic during publication".into(),
                ));
                return Err(error.into());
            }
        };
        let copied = match published {
            Ok(copied) => copied,
            Err(copier::CopyError::CrossDevice(_)) => {
                dispose_accepted_publication(accepted, pool, files, attempt).await?;
                accepted.failure = None;
                accepted.force_copy = true;
                continue;
            }
            Err(copier::CopyError::Publication { copied, error }) => {
                accepted.published = Some((location, *copied));
                return Err(error.into());
            }
            Err(error) => {
                return Err(error.into());
            }
        };
        accepted.published = Some((location, copied));
        if accepted.candidate.progress().cancel.is_cancelled() {
            dispose_accepted_publication(accepted, pool, files, attempt).await?;
            return Ok(Some(
                acquisition_failure(
                    copier::CopyError::Cancelled,
                    files,
                    accepted.library_id,
                    true,
                )
                .await,
            ));
        }
        return Ok(None);
    }
    Err(FinalisationError::SuffixExhausted.into())
}

async fn commit_accepted(
    accepted: &mut Accepted,
    config: &Config,
    pool: &PgPool,
    files: &LibraryFiles,
    attempt: Option<(&crate::models::ingestion_input::Input, Uuid)>,
) -> anyhow::Result<ProcessResult> {
    if accepted.recovering
        && let Some((input, job)) = attempt
    {
        use crate::models::ingestion_input::{AttemptOutcome, Publication, PublicationIdentity};
        let (location, copied) = accepted
            .published
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("publication evidence missing"))?;
        let publication = Publication {
            job,
            input_id: input.id,
            input_generation: input.generation,
            library_id: location.library_id.as_uuid(),
            path: location.path.as_str().into(),
            identity: sqlx::types::Json(PublicationIdentity {
                device: copied.identity.0,
                inode: copied.identity.1,
            }),
            hash: accepted.current_hash.clone(),
            size: i64::try_from(accepted.current_size)?,
            failure_class: None,
            failure_reason: None,
            imported: false,
        };
        let disposition =
            recover_publication(pool, files, &publication, PublicationRecovery::Inspect).await?;
        if disposition != PublicationDisposition::Verified {
            accepted.published = None;
            return Ok(ProcessResult::Operational(
                if disposition == PublicationDisposition::Absent {
                    AttemptOutcome::TransientInput
                } else {
                    AttemptOutcome::NeedsChange
                },
                "published candidate is absent or has changed ownership or content".into(),
            ));
        }
    }
    let (location, copied) = accepted
        .published
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("publication evidence missing"))?;
    let committed = commit_ingest_outcome(
        pool,
        accepted.extracted.as_ref(),
        &accepted.vars,
        location,
        copied,
        ManifestationMeta {
            format: ManifestationFormat::Epub,
            validation_status: accepted.validation_status,
            accessibility_metadata: &accepted.accessibility_metadata,
            has_embedded_cover: accepted.has_embedded_cover,
            current_hash: &accepted.current_hash,
            current_size: accepted.current_size,
        },
        attempt,
    )
    .await;
    match committed {
        Ok((work, manifestation)) => {
            accepted.created_directories.clear();
            tracing::info!(%work, %manifestation, "ingestion committed");
            warm_accepted(accepted, config, files, manifestation).await;
            Ok(ProcessResult::Complete)
        }
        Err(error) => {
            if attempt.is_some() {
                return Err(error.into());
            }
            let committed = sqlx::query!(
                "SELECT id, work_id FROM manifestations WHERE library_id = $1 AND file_path = $2 AND ingestion_file_hash = $3",
                location.library_id.as_uuid(), location.path.as_str(), &copied.sha256,
            ).fetch_optional(pool).await?;
            if let Some(row) = committed {
                warm_accepted(accepted, config, files, row.id).await;
                return Ok(ProcessResult::Complete);
            }
            discard_publication(pool, files, location, copied.identity).await?;
            accepted.published = None;
            Err(error.into())
        }
    }
}

async fn warm_accepted(
    accepted: &Accepted,
    config: &Config,
    files: &LibraryFiles,
    manifestation: Uuid,
) {
    if !should_warm_cover(ManifestationFormat::Epub, accepted.has_embedded_cover) {
        return;
    }
    let Some((location, _)) = &accepted.published else {
        return;
    };
    let phase_files = files.clone();
    let location = location.clone();
    match tokio::task::spawn_blocking(move || phase_files.open_source(&location)).await {
        Ok(Ok(opened)) => crate::services::covers::spawn_warm_thumb(
            config.library_path.as_str().to_owned(),
            manifestation,
            accepted.current_hash.clone(),
            opened.file,
        ),
        Ok(Err(error)) => tracing::warn!(%error, "cover warming source unavailable"),
        Err(error) => tracing::warn!(%error, "cover warming task failed"),
    }
}

fn checked_candidate(
    path: &Path,
) -> Result<RelativeFilePath, crate::services::files::LibraryFileError> {
    path.to_str()
        .ok_or(crate::services::files::LibraryFileError::InvalidLocation)?
        .parse()
}

fn copy_error_reason(error: &copier::CopyError) -> String {
    match error {
        copier::CopyError::Io(error)
        | copier::CopyError::DestinationIo(error)
        | copier::CopyError::Persist(crate::services::writeback::error::WritebackError::Io(
            error,
        ))
        | copier::CopyError::Publication {
            error: crate::services::writeback::error::WritebackError::Io(error),
            ..
        } => {
            format!("I/O error: {:?}", error.kind())
        }
        error => error.to_string(),
    }
}

/// Whether an ingest should pre-warm the cover thumbnail. EPUB is the only
/// format with an embedded cover, and a validated "no usable cover" makes the
/// warm a guaranteed wasted rasterization, since the validator reached that
/// verdict through the serve path's own extraction and rasterize routine.
/// Unknown (validator crashed, or no validator ran) still warms: serve is the
/// authority and a cold miss there is harmless.
fn should_warm_cover(format: ManifestationFormat, has_embedded_cover: Option<bool>) -> bool {
    matches!(format, ManifestationFormat::Epub) && has_embedded_cover != Some(false)
}

/// Manifestation-typed metadata for the initial insert, grouped so the
/// commit signature does not grow a parameter per new column.
struct ManifestationMeta<'a> {
    format: ManifestationFormat,
    validation_status: ValidationStatus,
    accessibility_metadata: &'a Option<serde_json::Value>,
    has_embedded_cover: Option<bool>,
    /// Hash and size of the library file as it exists now: the copy's values
    /// unless a repair rewrote the file.
    current_hash: &'a str,
    current_size: u64,
}

/// Run the ingest DB sequence atomically and return `(work_id, manifestation_id)`.
///
/// Sequence:
///   1. match work (if OPF has enough signal)
///   2. create stub work if no match
///   3. insert manifestation with NULL canonical + NULL pointers
///   4. write drafts (OPF drafts, or synthetic heuristic-title draft at 0.2)
///   5. upgrade stub work with pointers if newly created
///   6. UPDATE manifestation canonical values + pointer columns from draft IDs
#[expect(
    clippy::ref_option,
    reason = "commit_ingest is called with &extracted from a site that holds an owned Option; changing to Option<&T> would require .as_ref() at the call site with no readability benefit"
)]
#[cfg(test)]
async fn commit_ingest(
    pool: &PgPool,
    extracted: &Option<crate::services::metadata::extractor::ExtractedMetadata>,
    vars: &std::collections::HashMap<String, String>,
    location: &LibraryLocation,
    copy_result: &copier::CopyResult,
    meta: ManifestationMeta<'_>,
) -> Result<(Uuid, Uuid), sqlx::Error> {
    commit_ingest_outcome(
        pool,
        extracted.as_ref(),
        vars,
        location,
        copy_result,
        meta,
        None,
    )
    .await
}

async fn commit_ingest_outcome(
    pool: &PgPool,
    extracted: Option<&crate::services::metadata::extractor::ExtractedMetadata>,
    vars: &std::collections::HashMap<String, String>,
    location: &LibraryLocation,
    copy_result: &copier::CopyResult,
    meta: ManifestationMeta<'_>,
    attempt: Option<(&crate::models::ingestion_input::Input, Uuid)>,
) -> Result<(Uuid, Uuid), sqlx::Error> {
    use crate::services::metadata::draft;
    let mut tx = pool.begin().await?;

    // 1. Try to match an existing work (only when OPF gave us signal).
    let matched = match extracted.as_ref() {
        Some(meta) => work::match_existing(&mut tx, meta).await?,
        None => None,
    };

    let (work_id, was_created) = match matched {
        Some(id) => (id, false),
        None => (work::create_stub(&mut tx).await?, true),
    };

    // 2. Insert manifestation with NULL canonical + NULL pointers.
    //    `format`, `ingestion_status`, and `validation_status` are all bound as
    //    their typed Rust enums (sqlx::Type impls), so the write boundary stays
    //    symmetric with the read paths — a bad variant fails at the type system,
    //    not as a runtime Postgres cast error.
    let file_size =
        i64::try_from(meta.current_size).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let ingestion_status = IngestionStatus::Complete;
    let manifestation_id = sqlx::query_scalar!(
        "INSERT INTO manifestations \
             (work_id, format, file_path, ingestion_file_hash, current_file_hash, library_id, \
              file_size_bytes, ingestion_status, validation_status, accessibility_metadata, \
              has_embedded_cover) \
         VALUES ($1, $2, $3, $4, $5, $11, $6, $7, $8, $9, $10) \
         RETURNING id",
        work_id,
        meta.format as ManifestationFormat,
        location.path.as_str(),
        &copy_result.sha256,
        meta.current_hash,
        file_size,
        ingestion_status as IngestionStatus,
        meta.validation_status as ValidationStatus,
        meta.accessibility_metadata.as_ref(),
        meta.has_embedded_cover,
        location.library_id.as_uuid(),
    )
    .fetch_one(&mut *tx)
    .await?;

    if !library_path_claim::reserve(&mut tx, location, manifestation_id).await? {
        return Err(sqlx::Error::Protocol(
            "ingestion destination belongs to another manifestation".into(),
        ));
    }

    // 3. Write drafts — OPF metadata when available, heuristic fallback otherwise.
    //    The heuristic row gives the canonical title_version_id pointer even
    //    when no OPF metadata exists, preserving the ingest invariant.
    let metadata_for_drafts = draft_metadata(extracted, vars);
    let draft_ids = draft::write_drafts(&mut tx, manifestation_id, &metadata_for_drafts).await?;

    // 4. Upgrade stub work with real values + pointers (create path only).
    if was_created {
        work::upgrade_stub(&mut tx, work_id, &metadata_for_drafts, &draft_ids).await?;
    }

    // 5. Populate manifestation canonical columns + *_version_id pointers
    //    from OPF extraction (not the heuristic row — only real OPF values
    //    become canonical ISBN/publisher/pub_date).
    let (isbn_10, isbn_13) = extracted
        .as_ref()
        .and_then(|m| m.isbn.as_ref())
        .map_or((None, None), |i| (i.isbn_10.clone(), i.isbn_13.clone()));
    let publisher = extracted.as_ref().and_then(|m| m.publisher.clone());
    let pub_date = extracted.as_ref().and_then(|m| m.pub_date);
    let pages = extracted.as_ref().and_then(|m| m.pages);

    sqlx::query!(
        "UPDATE manifestations SET \
            isbn_10 = $1, isbn_13 = $2, publisher = $3, pub_date = $4, pages = $5, \
            isbn_10_version_id = $6, isbn_13_version_id = $7, \
            publisher_version_id = $8, pub_date_version_id = $9, pages_version_id = $10 \
         WHERE id = $11",
        isbn_10.as_deref(),
        isbn_13.as_deref(),
        publisher.as_deref(),
        pub_date,
        pages,
        draft_ids.get("isbn_10").copied(),
        draft_ids.get("isbn_13").copied(),
        draft_ids.get("publisher").copied(),
        draft_ids.get("pub_date").copied(),
        draft_ids.get("pages").copied(),
        manifestation_id,
    )
    .execute(&mut *tx)
    .await?;

    if let Some((input, job)) = attempt {
        crate::models::ingestion_input::finish(
            &mut tx,
            input,
            job,
            crate::models::ingestion_input::AttemptOutcome::Imported,
            crate::models::ingestion_input::InputStatus::Imported,
            None,
            Some(work_id),
        )
        .await?;
    }
    tx.commit().await?;
    Ok((work_id, manifestation_id))
}

fn draft_metadata(
    extracted: Option<&crate::services::metadata::extractor::ExtractedMetadata>,
    vars: &std::collections::HashMap<String, String>,
) -> crate::services::metadata::extractor::ExtractedMetadata {
    use crate::services::metadata::extractor::ExtractedMetadata;
    extracted.map_or_else(
        || {
            let title = vars
                .get("Title")
                .cloned()
                .unwrap_or_else(|| "Unknown".into());
            ExtractedMetadata {
                title: Some(title.clone()),
                sort_title: Some(title),
                subtitle: None,
                description: None,
                language: None,
                creators: Vec::new(),
                unmapped_contributors: Vec::new(),
                pages: None,
                publisher: None,
                pub_date: None,
                isbn: None,
                subjects: Vec::new(),
                series: None,
                inversion: None,
                confidence: 0.2,
            }
        },
        ExtractedMetadata::clone,
    )
}

#[cfg(test)]
#[expect(
    clippy::items_after_statements,
    reason = "test code: local struct definitions inside test functions are idiomatic for sqlx::FromRow test helpers"
)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_review_fresh_shared_recommit_keeps_pause_until_healthy_probe(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input;
        use std::os::unix::fs::PermissionsExt;
        let ing = ingestion_pool_for(&pool).await;
        let (source, library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.probe().await.unwrap();
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &owner.config,
            &ing,
            &owner.files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        assert!(matches!(result, ProcessResult::Accepted(_)));
        owner.pending = Some(PendingResult {
            input: input.clone(),
            job,
            result,
            source_deleted: false,
            health: FinalisationHealth::NoFreshFailure,
        });
        owner.pause(&"previous dependency outage");
        owner.pause.as_mut().unwrap().next = tokio::time::Instant::now();
        let mut active = owner.reprobe().await.unwrap().unwrap();
        std::fs::set_permissions(library.path(), std::fs::Permissions::from_mode(0o0)).unwrap();
        let completed = (&mut active.future).await;
        std::fs::set_permissions(library.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(completed.health, FinalisationHealth::SharedFailure);
        owner.complete_attempt(completed, true).await;
        assert!(owner.pending.is_none());
        assert!(owner.pause.is_some());
        assert!(
            owner
                .start_attempt(&CancellationToken::new())
                .await
                .unwrap()
                .is_none()
        );
        owner.pause.as_mut().unwrap().next = tokio::time::Instant::now();
        assert!(owner.reprobe().await.unwrap().is_none());
        assert!(owner.pause.is_none());
        assert!(source.path().join("book.epub").exists());
        assert_eq!(
            ingestion_input::transient_count(&ing, &input)
                .await
                .unwrap(),
            0
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_discovery_watcher_flood_services_completion_and_shutdown(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let cancel = CancellationToken::new();
        let (handle, commands) = coordinator_channel();
        let settings = crate::test_support::test_settings();
        settings.write().await.ingestion.cleanup_imported = false;
        let worker = tokio::spawn(run_watcher(
            config,
            ing.clone(),
            cancel.clone(),
            files,
            settings,
            commands,
        ));
        handle.scan().await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let flood_cancel = CancellationToken::new();
        let stop = flood_cancel.clone();
        let flood_path = source.path().join("traffic.pdf");
        let flood = tokio::spawn(async move {
            while !stop.is_cancelled() {
                std::fs::write(&flood_path, b"traffic").unwrap();
                std::fs::remove_file(&flood_path).unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                if ingestion_input::current(&ing, input.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status
                    == ingestion_input::InputStatus::Imported
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        flood_cancel.cancel();
        flood.await.unwrap();
        assert!(source.path().join("book.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_discovery_targeted_paths_and_directory_rename(pool: PgPool) {
        use crate::models::ingestion_input::{self, InputStatus};
        use std::os::unix::fs::PermissionsExt;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::create_dir_all(source.path().join("tree")).unwrap();
        std::fs::write(source.path().join("tree/book.epub"), b"original").unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.discover(false).await.unwrap();
        let original = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let observed = owner.observed.get(b"tree/book.epub".as_slice()).unwrap().1;
        std::fs::write(source.path().join("unrelated.epub"), b"not signalled").unwrap();
        owner
            .observe_paths(vec![source.path().join("tree/book.epub")])
            .await
            .unwrap();
        let unchanged = ingestion_input::current(&ing, original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.observed_at, original.observed_at);
        assert_eq!(
            owner.observed.get(b"tree/book.epub".as_slice()).unwrap().1,
            observed
        );
        assert_eq!(
            ingestion_input::current_page(&ing, None)
                .await
                .unwrap()
                .len(),
            1
        );
        std::fs::rename(source.path().join("tree"), source.path().join("renamed")).unwrap();
        owner
            .observe_paths(vec![
                source.path().join("tree"),
                source.path().join("renamed"),
                source.path().join("renamed/book.epub"),
            ])
            .await
            .unwrap();
        assert!(
            ingestion_input::current(&ing, original.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!owner.observed.contains_key(b"tree/book.epub".as_slice()));
        let renamed = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(renamed.source_path, b"renamed/book.epub");
        assert_eq!(renamed.status, InputStatus::Pending);
        std::fs::set_permissions(
            source.path().join("renamed"),
            std::fs::Permissions::from_mode(0o0),
        )
        .unwrap();
        owner
            .observe_paths(vec![source.path().join("renamed")])
            .await
            .unwrap();
        std::fs::set_permissions(
            source.path().join("renamed"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(
            ingestion_input::current(&ing, renamed.id)
                .await
                .unwrap()
                .is_some()
        );
        owner.discover(true).await.unwrap();
        assert_eq!(
            ingestion_input::current_page(&ing, None)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_discovery_finalisation_refreshes_only_changed_source(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.probe().await.unwrap();
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &owner.config,
            &ing,
            &owner.files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        std::fs::write(
            source.path().join("book.epub"),
            b"new generation without notify",
        )
        .unwrap();
        std::fs::write(source.path().join("unrelated.epub"), b"not signalled").unwrap();
        owner.pending = Some(PendingResult {
            input: input.clone(),
            job,
            result,
            source_deleted: false,
            health: FinalisationHealth::NoFreshFailure,
        });
        owner.finalise().await.unwrap();
        let current = ingestion_input::current(&ing, input.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.generation, input.generation + 1);
        assert_eq!(current.status, ingestion_input::InputStatus::Pending);
        assert_eq!(
            ingestion_input::current_page(&ing, None)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(owner.keys.contains_key(&input.id));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_discovery_failed_preparation_prunes_only_created_parents(
        pool: PgPool,
    ) {
        let ing = ingestion_pool_for(&pool).await;
        let (source, library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::create_dir(library.path().join("existing")).unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let ProcessResult::Accepted(mut accepted) = process_file(
            &source.path().join("book.epub"),
            &config,
            &ing,
            &files,
            library_id,
            None,
            copier::Progress::new(&CancellationToken::new()),
        )
        .await
        else {
            panic!("expected accepted input")
        };
        accepted.path = format!("existing/created/{}.epub", "x".repeat(260))
            .parse()
            .unwrap();
        assert!(matches!(
            finish_accepted(&mut accepted, &config, &ing, &files, None)
                .await
                .unwrap(),
            ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::NeedsChange,
                _
            )
        ));
        assert!(library.path().join("existing").is_dir());
        assert!(!library.path().join("existing/created").exists());
        assert!(source.path().join("book.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_discovery_disposal_preserves_retained_entries(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let (source, library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        let (input, job, mut accepted, publication) =
            published_attempt(&mut owner, "book.epub").await;
        let parent = library
            .path()
            .join(&publication.path)
            .parent()
            .unwrap()
            .to_owned();
        std::fs::write(parent.join(".DS_Store"), b"preserve library metadata").unwrap();
        dispose_accepted_publication(&mut accepted, &ing, &owner.files, Some((&input, job)))
            .await
            .unwrap();
        assert!(!library.path().join(&publication.path).exists());
        assert!(parent.join(".DS_Store").is_file());
        assert!(source.path().join("book.epub").exists());
    }

    async fn published_attempt(
        owner: &mut Coordinator,
        name: &str,
    ) -> (
        crate::models::ingestion_input::Input,
        Uuid,
        Box<Accepted>,
        crate::models::ingestion_input::Publication,
    ) {
        use crate::models::ingestion_input;
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&owner.pool, None)
            .await
            .unwrap()
            .into_iter()
            .find(|input| input.source_path == name.as_bytes())
            .unwrap();
        let job = ingestion_input::begin_attempt(&owner.pool, &input, Uuid::new_v4())
            .await
            .unwrap();
        let library_id = crate::models::storage_library::default_library_id(&owner.pool)
            .await
            .unwrap();
        let result = process_file(
            &owner.config.ingestion_path.as_path().join(name),
            &owner.config,
            &owner.pool,
            &owner.files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        let ProcessResult::Accepted(mut accepted) = result else {
            panic!("expected accepted input")
        };
        assert!(
            publish_accepted(
                &mut accepted,
                &owner.pool,
                &owner.files,
                Some((&input, job))
            )
            .await
            .unwrap()
            .is_none()
        );
        let publication = ingestion_input::publication_page(&owner.pool, None)
            .await
            .unwrap()
            .into_iter()
            .find(|publication| publication.job == job)
            .unwrap();
        (input, job, accepted, publication)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_live_retry_preserves_changed_bytes(pool: PgPool) {
        use crate::models::ingestion_input::{self, AttemptOutcome};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let (input, job, mut accepted, publication) =
            published_attempt(&mut owner, "book.epub").await;
        let root = owner.files.library(library_id).unwrap();
        let foreign = vec![b'x'; usize::try_from(publication.size).unwrap()];
        root.write(&publication.path, &foreign).unwrap();
        let result = finish_accepted(
            &mut accepted,
            &owner.config,
            &ing,
            &owner.files,
            Some((&input, job)),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            ProcessResult::Operational(AttemptOutcome::NeedsChange, _)
        ));
        assert_eq!(root.read(&publication.path).unwrap(), foreign);
        assert!(
            ingestion_input::publication_page(&ing, None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            sqlx::query!("SELECT id FROM manifestations")
                .fetch_all(&ing)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_recovery_preserves_foreign_identity_hash_and_size(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        for mode in ["absent", "identity", "hash", "size"] {
            let name = format!("{mode}.epub");
            std::fs::write(source.path().join(&name), make_minimal_epub()).unwrap();
            let (input, _job, _accepted, publication) = published_attempt(&mut owner, &name).await;
            let root = owner.files.library(library_id).unwrap();
            match mode {
                "absent" => root.remove_file(&publication.path).unwrap(),
                "identity" => {
                    root.rename(&publication.path, root, "saved-original.epub")
                        .unwrap();
                    root.write(&publication.path, b"FOREIGN").unwrap();
                }
                "hash" => root
                    .write(
                        &publication.path,
                        vec![0; usize::try_from(publication.size).unwrap()],
                    )
                    .unwrap(),
                _ => {
                    use std::io::Write;
                    root.open_with(
                        &publication.path,
                        cap_std::fs::OpenOptions::new().append(true),
                    )
                    .unwrap()
                    .write_all(b"changed")
                    .unwrap();
                }
            }
            let expected = if mode == "absent" {
                None
            } else {
                Some(root.read(&publication.path).unwrap())
            };
            recover_publication(
                &ing,
                &owner.files,
                &publication,
                PublicationRecovery::Startup,
            )
            .await
            .unwrap();
            assert!(
                ingestion_input::publication_page(&ing, None)
                    .await
                    .unwrap()
                    .is_empty()
            );
            if let Some(expected) = expected {
                assert_eq!(root.read(&publication.path).unwrap(), expected);
                assert_eq!(
                    ingestion_input::current(&ing, input.id)
                        .await
                        .unwrap()
                        .unwrap()
                        .status,
                    ingestion_input::InputStatus::OperationalFailure
                );
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_local_obstruction_does_not_block_another_input(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, AttemptOutcome};
        use std::os::unix::fs::PermissionsExt;
        let ing = ingestion_pool_for(&pool).await;
        let (source, library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.settings.write().await.ingestion.cleanup_imported = false;
        std::fs::write(source.path().join("blocked.epub"), make_minimal_epub()).unwrap();
        let (input, job, mut accepted, publication) =
            published_attempt(&mut owner, "blocked.epub").await;
        let parent = library
            .path()
            .join(std::path::Path::new(&publication.path).parent().unwrap());
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let restore = Restore(parent.clone());
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500)).unwrap();
        accepted.failure = Some((
            AttemptOutcome::TransientInput,
            "publication durability failed".into(),
        ));
        owner.pending = Some(PendingResult {
            input: input.clone(),
            job,
            result: ProcessResult::Accepted(accepted),
            source_deleted: false,
            health: FinalisationHealth::NoFreshFailure,
        });
        owner.finalise().await.unwrap();
        assert!(owner.pending.is_none());
        assert!(owner.unresolved.contains(&input.id));
        assert!(owner.pause.is_none());
        std::fs::write(
            source.path().join("Healthy - Other.epub"),
            make_minimal_epub(),
        )
        .unwrap();
        owner.discover(false).await.unwrap();
        let other = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .into_iter()
            .find(|input| input.source_path == b"Healthy - Other.epub")
            .unwrap();
        attempt(&mut owner, other.clone()).await;
        assert_eq!(
            ingestion_input::current(&ing, other.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            ingestion_input::InputStatus::Imported
        );
        assert_eq!(
            ingestion_input::publication_page(&ing, None)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            owner
                .files
                .library(library_id)
                .unwrap()
                .try_exists(&publication.path)
                .unwrap()
        );
        drop(restore);
        owner
            .observe_paths(vec![source.path().join("Healthy - Other.epub")])
            .await
            .unwrap();
        assert_eq!(
            ingestion_input::publication_page(&ing, None)
                .await
                .unwrap()
                .len(),
            1
        );
        owner
            .observe_paths(vec![source.path().join("blocked.epub")])
            .await
            .unwrap();
        assert!(
            ingestion_input::publication_page(&ing, None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!owner.unresolved.contains(&input.id));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_constraint_failure_keeps_the_committed_peer(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, AttemptOutcome, InputStatus};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let (input, job, mut accepted, publication) =
            published_attempt(&mut owner, "book.epub").await;
        let peer = LibraryLocation {
            library_id,
            path: "peer.epub".parse().unwrap(),
        };
        let root = owner.files.library(library_id).unwrap();
        let bytes = root.read(&publication.path).unwrap();
        root.write(peer.path.as_path(), &bytes).unwrap();
        let copied = accepted.published.as_ref().unwrap().1.clone();
        commit_ingest(
            &ing,
            &None,
            &accepted.vars,
            &peer,
            &copied,
            ManifestationMeta {
                format: ManifestationFormat::Epub,
                validation_status: accepted.validation_status,
                accessibility_metadata: &accepted.accessibility_metadata,
                has_embedded_cover: accepted.has_embedded_cover,
                current_hash: &accepted.current_hash,
                current_size: accepted.current_size,
            },
        )
        .await
        .unwrap();
        let result = finish_accepted(
            &mut accepted,
            &owner.config,
            &ing,
            &owner.files,
            Some((&input, job)),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            ProcessResult::Operational(AttemptOutcome::NeedsChange, _)
        ));
        assert!(!root.try_exists(&publication.path).unwrap());
        assert_eq!(root.read(peer.path.as_path()).unwrap(), bytes);
        assert!(
            ingestion_input::publication_page(&ing, None)
                .await
                .unwrap()
                .is_empty()
        );
        owner.pending = Some(PendingResult {
            input: input.clone(),
            job,
            result,
            source_deleted: false,
            health: FinalisationHealth::NoFreshFailure,
        });
        owner.finalise().await.unwrap();
        assert_eq!(
            ingestion_input::current(&ing, input.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            InputStatus::OperationalFailure
        );
        assert!(!owner.keys.contains_key(&input.id));
        assert!(source.path().join("book.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_exact_job_acknowledgement_survives_relocation(
        pool: PgPool,
    ) {
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let (input, job, mut accepted, publication) =
            published_attempt(&mut owner, "book.epub").await;
        assert!(matches!(
            finish_accepted(
                &mut accepted,
                &owner.config,
                &ing,
                &owner.files,
                Some((&input, job))
            )
            .await
            .unwrap(),
            ProcessResult::Complete
        ));
        let previous = accepted.published.as_ref().unwrap().0.clone();
        let destination = LibraryLocation {
            library_id,
            path: "moved.epub".parse().unwrap(),
        };
        let mut tx = ing.begin().await.unwrap();
        let manifestation = library_path_claim::owner(&mut tx, &previous)
            .await
            .unwrap()
            .unwrap();
        assert!(
            library_path_claim::reserve(&mut tx, &destination, manifestation)
                .await
                .unwrap()
        );
        library_path_claim::exclude(&mut tx, &previous)
            .await
            .unwrap();
        let root = owner.files.library(library_id).unwrap();
        path_rename::move_existing(root, &previous.path, &destination.path, &publication.hash)
            .unwrap();
        sqlx::query!(
            "UPDATE manifestations SET file_path = $2 WHERE id = $1",
            manifestation,
            destination.path.as_str()
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        library_path_claim::release_obsolete(&mut tx, manifestation)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        root.write(previous.path.as_path(), b"FOREIGN").unwrap();
        let bytes = root.read(destination.path.as_path()).unwrap();
        assert!(matches!(
            finish_accepted(
                &mut accepted,
                &owner.config,
                &ing,
                &owner.files,
                Some((&input, job))
            )
            .await
            .unwrap(),
            ProcessResult::Complete
        ));
        assert_eq!(root.read(previous.path.as_path()).unwrap(), b"FOREIGN");
        assert_eq!(root.read(destination.path.as_path()).unwrap(), bytes);
        assert_eq!(
            path_rename::parent(root, &previous.path)
                .unwrap()
                .0
                .entries()
                .unwrap()
                .map(Result::unwrap)
                .filter(|entry| entry.file_type().unwrap().is_file())
                .count(),
            1
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_review_local_cleanup_denial_releases_completed_attempt(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, InputStatus};
        use std::os::unix::fs::PermissionsExt;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &owner.config,
            &ing,
            &owner.files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        owner.pending = Some(PendingResult {
            input: input.clone(),
            job,
            result,
            source_deleted: false,
            health: FinalisationHealth::NoFreshFailure,
        });
        std::fs::set_permissions(source.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let finalised = owner.finalise().await;
        std::fs::set_permissions(source.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            finalised.is_ok(),
            "local cleanup must preserve completion and release ownership: {finalised:?}"
        );
        assert!(owner.pending.is_none());
        assert!(source.path().join("book.epub").exists());
        assert_eq!(
            ingestion_input::current(&ing, input.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            InputStatus::Imported
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_review_long_final_name_needs_change(pool: PgPool) {
        use crate::models::ingestion_input::{self, AttemptOutcome};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &owner.config,
            &ing,
            &owner.files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        let ProcessResult::Accepted(mut accepted) = result else {
            panic!("expected accepted input")
        };
        accepted.path = format!("{}.epub", "x".repeat(300)).parse().unwrap();
        let completed = finish_accepted(
            &mut accepted,
            &owner.config,
            &ing,
            &owner.files,
            Some((&input, job)),
        )
        .await;
        assert!(
            matches!(
                completed,
                Ok(ProcessResult::Operational(AttemptOutcome::NeedsChange, _))
            ),
            "a healthy-root name error must be terminal for the input"
        );
        assert!(source.path().join("book.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_review_successful_delete_receipt_survives_database_failure(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.settings.write().await.ingestion.cleanup_imported = false;
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        attempt(&mut owner, input.clone()).await;
        std::fs::remove_file(source.path().join("book.epub")).unwrap();
        owner.pending = Some(PendingResult {
            input: input.clone(),
            job: Uuid::new_v4(),
            result: ProcessResult::Complete,
            source_deleted: true,
            health: FinalisationHealth::NoFreshFailure,
        });
        ing.close().await;
        assert!(owner.finalise().await.is_err());
        assert!(owner.pending.as_ref().unwrap().source_deleted);
        owner.pool = ingestion_pool_for(&pool).await;
        owner.finalise().await.unwrap();
        assert!(owner.pending.is_none());
        assert!(
            ingestion_input::current(&owner.pool, input.id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_review_restart_records_unattributed_removal(pool: PgPool) {
        use crate::models::ingestion_input::{self, InputPath};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config.clone(), files.clone());
        owner.settings.write().await.ingestion.cleanup_imported = false;
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        attempt(&mut owner, input.clone()).await;
        assert!(
            cleanup::remove_verified(
                files.ingestion(),
                &InputPath::from_bytes(input.source_path.clone()).unwrap(),
                &input.fingerprint.0,
            )
            .unwrap()
        );
        owner.pending = Some(PendingResult {
            input: input.clone(),
            job: Uuid::new_v4(),
            result: ProcessResult::Complete,
            source_deleted: true,
            health: FinalisationHealth::NoFreshFailure,
        });
        ing.close().await;
        assert!(owner.finalise().await.is_err());
        assert!(owner.pending.as_ref().unwrap().source_deleted);
        drop(owner);
        let restarted_pool = ingestion_pool_for(&pool).await;
        let mut restarted = coordinator(restarted_pool.clone(), config, files);
        restarted.probe().await.unwrap();
        restarted.discover(false).await.unwrap();
        let removed = sqlx::query!(
            "SELECT removal_cause, removed_at FROM ingestion_inputs WHERE id = $1",
            input.id
        )
        .fetch_one(&restarted_pool)
        .await
        .unwrap();
        assert!(removed.removed_at.is_some());
        assert_eq!(
            removed.removal_cause.as_deref(),
            Some("unattributed_disappearance")
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_review_shutdown_publication_recovers_without_second_copy(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config.clone(), files.clone());
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &config,
            &ing,
            &files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        let ProcessResult::Accepted(mut accepted) = result else {
            panic!("expected accepted input")
        };
        assert!(
            publish_accepted(&mut accepted, &ing, &files, Some((&input, job)))
                .await
                .unwrap()
                .is_none()
        );
        let location = accepted.published.as_ref().unwrap().0.clone();
        ing.close().await;
        assert!(
            finish_accepted(&mut accepted, &config, &ing, &files, Some((&input, job)))
                .await
                .is_err()
        );
        drop(accepted);
        drop(owner);
        let resumed = ingestion_pool_for(&pool).await;
        let mut restarted = coordinator(resumed.clone(), config, files);
        restarted.probe().await.unwrap();
        restarted.discover(false).await.unwrap();
        let reclaimed = ingestion_input::current(&resumed, input.id)
            .await
            .unwrap()
            .unwrap();
        attempt(&mut restarted, reclaimed).await;
        let root = restarted.files.library(library_id).unwrap();
        let (parent, _) = path_rename::parent(root, &location.path).unwrap();
        assert_eq!(
            parent.entries().unwrap().count(),
            1,
            "restart must not leave the unregistered first copy"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_review_deterministic_registration_error_needs_change(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, AttemptOutcome};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config.clone(), files.clone());
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let _job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &config,
            &ing,
            &files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        let ProcessResult::Accepted(mut accepted) = result else {
            panic!("expected accepted input")
        };
        let result = finish_accepted(
            &mut accepted,
            &config,
            &ing,
            &files,
            Some((&input, Uuid::new_v4())),
        )
        .await;
        assert!(
            matches!(
                result,
                Ok(ProcessResult::Operational(AttemptOutcome::NeedsChange, _))
            ),
            "repeatable registration failure must not stay accepted for endless recommit"
        );
        assert!(accepted.published.is_none());
        assert!(source.path().join("book.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_recommit_advances_probe_deadline_before_dispatch(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input;
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.probe().await.unwrap();
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &owner.config,
            &ing,
            &owner.files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        assert!(matches!(result, ProcessResult::Accepted(_)));
        owner.pending = Some(PendingResult {
            input,
            job,
            result,
            source_deleted: false,
            health: FinalisationHealth::NoFreshFailure,
        });
        owner.pause(&"outcome commit unavailable");
        owner.pause.as_mut().unwrap().next = tokio::time::Instant::now();
        let mut active = owner.reprobe().await.unwrap().unwrap();
        assert!(active.recommit);
        assert_eq!(owner.pause.as_ref().unwrap().step, 1);
        assert!(
            owner
                .pause
                .as_ref()
                .unwrap()
                .next
                .duration_since(tokio::time::Instant::now())
                >= std::time::Duration::from_secs(59)
        );
        let completed = (&mut active.future).await;
        assert!(matches!(completed.result, ProcessResult::Complete));
        owner.pending = Some(completed);
        owner.finalise().await.unwrap();
        assert!(owner.pending.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_needs_change_resets_on_admin_startup_and_generation(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, AttemptOutcome, InputStatus};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        owner.discover(false).await.unwrap();
        for reset in ["admin", "startup", "generation"] {
            let input = ingestion_input::current_page(&ing, None)
                .await
                .unwrap()
                .remove(0);
            let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
                .await
                .unwrap();
            let mut tx = ing.begin().await.unwrap();
            ingestion_input::finish(
                &mut tx,
                &input,
                job,
                AttemptOutcome::NeedsChange,
                InputStatus::OperationalFailure,
                Some("PermissionDenied"),
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            assert_eq!(owner.discover(false).await.unwrap().suppressed, 1);
            assert!(!owner.keys.contains_key(&input.id));
            match reset {
                "admin" => {
                    owner.discover(true).await.unwrap();
                }
                "startup" => {
                    ingestion_input::reclaim(&ing).await.unwrap();
                    owner.discover(false).await.unwrap();
                }
                _ => {
                    std::fs::write(source.path().join("book.epub"), b"new generation").unwrap();
                    owner.discover(false).await.unwrap();
                }
            }
            let reset_input = ingestion_input::current(&ing, input.id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(reset_input.status, InputStatus::Pending);
            assert!(reset_input.retry_reset_at > input.retry_reset_at);
            let retry = ingestion_input::retry_states(&ing, &[input.id])
                .await
                .unwrap()
                .remove(0);
            assert!(!retry.needs_change);
            assert_eq!(retry.count, 0);
            assert!(owner.keys.contains_key(&input.id));
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_seed_running_worker_obeys_readiness_and_live_settings(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, InputStatus};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, mut config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        config.accepted_formats.clear();
        config.cleanup_imported = true;
        let seeded = crate::services::settings::seed_ingestion(&pool, &config)
            .await
            .unwrap();
        let settings = std::sync::Arc::new(tokio::sync::RwLock::new(seeded));
        let cancel = CancellationToken::new();
        let (handle, commands) = coordinator_channel();
        let worker = tokio::spawn(run_watcher(
            config,
            ing.clone(),
            cancel.clone(),
            files,
            settings.clone(),
            commands,
        ));
        let discovery = handle.scan().await.unwrap();
        assert_eq!(discovery.suppressed, 1);
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(input.status, InputStatus::NotAccepted);
        let update =
            serde_json::from_value(serde_json::json!({"accepted_formats": ["epub"]})).unwrap();
        let saved = crate::services::settings::save(&pool, &update)
            .await
            .unwrap();
        crate::services::settings::apply_if_newer(&mut *settings.write().await, saved);
        let discovery = handle.scan().await.unwrap();
        assert_eq!(discovery.deferred, 1);
        assert_eq!(
            sqlx::query_scalar!(
                r#"SELECT COUNT(*) AS "count!" FROM ingestion_jobs WHERE input_id = $1"#,
                input.id
            )
            .fetch_one(&ing)
            .await
            .unwrap(),
            0
        );
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                if ingestion_input::current(&ing, input.id)
                    .await
                    .unwrap()
                    .is_none()
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        assert!(!source.path().join("book.epub").exists());
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT outcome::text FROM ingestion_jobs WHERE input_id = $1",
                input.id
            )
            .fetch_one(&ing)
            .await
            .unwrap(),
            Some("imported".into())
        );
        cancel.cancel();
        worker.await.unwrap().unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_shutdown_has_no_outcome_and_startup_reclaims(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, InputStatus};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config.clone(), files.clone());
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let cancel = CancellationToken::new();
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let mut active = owner
            .active_attempt(input.clone(), job, library_id, &cancel)
            .unwrap();
        cancel.cancel();
        let result = (&mut active.future).await;
        assert!(matches!(result.result, ProcessResult::Operational(_, _)));
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT outcome::text FROM ingestion_jobs WHERE id = $1",
                job
            )
            .fetch_one(&ing)
            .await
            .unwrap(),
            None
        );
        assert!(source.path().join("book.epub").exists());
        assert_eq!(
            ingestion_input::current(&ing, input.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            InputStatus::Processing
        );
        ingestion_input::reclaim(&ing).await.unwrap();
        let reclaimed = ingestion_input::current(&ing, input.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reclaimed.status, InputStatus::Pending);
        assert!(reclaimed.retry_reset_at > input.retry_reset_at);
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT outcome::text FROM ingestion_jobs WHERE id = $1",
                job
            )
            .fetch_one(&ing)
            .await
            .unwrap(),
            Some("interrupted".into())
        );
        assert_eq!(
            ingestion_input::transient_count(&ing, &reclaimed)
                .await
                .unwrap(),
            0
        );
    }

    async fn attempt(owner: &mut Coordinator, input: crate::models::ingestion_input::Input) {
        let library_id = crate::models::storage_library::default_library_id(&owner.pool)
            .await
            .unwrap();
        let path = owner.config.ingestion_path.as_path().join(
            crate::models::ingestion_input::InputPath::from_bytes(input.source_path.clone())
                .unwrap()
                .path(),
        );
        let job =
            crate::models::ingestion_input::begin_attempt(&owner.pool, &input, Uuid::new_v4())
                .await
                .unwrap();
        let result = process_file(
            &path,
            &owner.config,
            &owner.pool,
            &owner.files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        owner.pending = Some(PendingResult {
            input,
            job,
            result,
            source_deleted: false,
            health: FinalisationHealth::NoFreshFailure,
        });
        owner.finalise().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_discovery_rename_missing_and_partial_scan(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, InputStatus};
        use std::os::unix::fs::PermissionsExt;
        let pool = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        std::fs::write(source.path().join("cover.jpg"), b"sidecar").unwrap();
        std::fs::write(source.path().join(".hidden.epub"), b"ignore").unwrap();
        std::fs::write(source.path().join("Thumbs.db"), b"ignore").unwrap();
        let mut owner = coordinator(pool.clone(), config, files);
        let discovered = owner.discover(false).await.unwrap();
        assert_eq!(
            (
                discovered.queued,
                discovered.deferred,
                discovered.suppressed
            ),
            (0, 1, 1)
        );
        assert_eq!(owner.discover(true).await.unwrap().deferred, 1);
        assert_eq!(owner.keys.len(), 1);
        let old = ingestion_input::current_page(&pool, None)
            .await
            .unwrap()
            .into_iter()
            .find(|input| input.source_path == b"book.epub")
            .unwrap();
        std::fs::rename(
            source.path().join("book.epub"),
            source.path().join("renamed.epub"),
        )
        .unwrap();
        std::fs::remove_file(source.path().join("cover.jpg")).unwrap();
        owner.discover(false).await.unwrap();
        assert!(
            ingestion_input::current(&pool, old.id)
                .await
                .unwrap()
                .is_none()
        );
        let present = ingestion_input::current_page(&pool, None).await.unwrap();
        assert_eq!(present.len(), 1);
        assert_eq!(present[0].source_path, b"renamed.epub");
        assert_ne!(present[0].id, old.id);
        assert_eq!(present[0].status, InputStatus::Pending);
        std::fs::set_permissions(source.path(), std::fs::Permissions::from_mode(0o000)).unwrap();
        let partial = owner.discover(false).await;
        std::fs::set_permissions(source.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        partial.unwrap();
        assert!(
            ingestion_input::current(&pool, present[0].id)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_import_duplicate_rejection_cleanup_and_restart_suppression(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, InputStatus};
        let pool = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let bytes = make_minimal_epub();
        std::fs::write(source.path().join("book.epub"), &bytes).unwrap();
        let mut owner = coordinator(pool.clone(), config, files);
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&pool, None)
            .await
            .unwrap()
            .remove(0);
        let imported_id = input.id;
        attempt(&mut owner, input).await;
        assert!(
            ingestion_input::current(&pool, imported_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!source.path().join("book.epub").exists());
        std::fs::write(source.path().join("duplicate.epub"), &bytes).unwrap();
        owner.discover(false).await.unwrap();
        let duplicate = ingestion_input::current_page(&pool, None)
            .await
            .unwrap()
            .remove(0);
        let duplicate_id = duplicate.id;
        attempt(&mut owner, duplicate).await;
        let duplicate = ingestion_input::current(&pool, duplicate_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(duplicate.status, InputStatus::Duplicate);
        assert!(duplicate.work_id.is_some());
        assert!(source.path().join("duplicate.epub").exists());
        owner.settings.write().await.ingestion.cleanup_duplicates = true;
        std::fs::write(source.path().join("duplicate-opt-in.epub"), &bytes).unwrap();
        owner.discover(false).await.unwrap();
        let opt_in = ingestion_input::current_page(&pool, None)
            .await
            .unwrap()
            .into_iter()
            .find(|input| input.source_path == b"duplicate-opt-in.epub")
            .unwrap();
        let opt_in_id = opt_in.id;
        attempt(&mut owner, opt_in).await;
        assert!(
            ingestion_input::current(&pool, opt_in_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!source.path().join("duplicate-opt-in.epub").exists());
        std::fs::write(source.path().join("bad.epub"), b"corrupt").unwrap();
        std::fs::write(source.path().join("unaccepted.pdf"), b"PDF").unwrap();
        owner.discover(false).await.unwrap();
        let rejected = ingestion_input::current_page(&pool, None)
            .await
            .unwrap()
            .into_iter()
            .find(|input| input.source_path == b"bad.epub")
            .unwrap();
        let rejected_id = rejected.id;
        attempt(&mut owner, rejected).await;
        let rejected = ingestion_input::current(&pool, rejected_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rejected.status, InputStatus::Rejected);
        assert!(
            rejected
                .reason
                .as_deref()
                .unwrap()
                .contains("EPUB rejected")
        );
        assert_eq!(
            std::fs::read(source.path().join("bad.epub")).unwrap(),
            b"corrupt"
        );
        ingestion_input::reclaim(&pool).await.unwrap();
        owner.observed.clear();
        assert_eq!(owner.discover(true).await.unwrap().suppressed, 3);
        assert_eq!(
            ingestion_input::current(&pool, rejected_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            InputStatus::Rejected
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_retains_completed_result_when_database_unavailable_and_recommits(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, InputStatus};
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let mut owner = coordinator(ing.clone(), config.clone(), files.clone());
        owner.discover(false).await.unwrap();
        let input = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let result = process_file(
            &source.path().join("book.epub"),
            &config,
            &ing,
            &files,
            library_id,
            Some(&input),
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        let ProcessResult::Accepted(mut accepted) = result else {
            panic!("expected independently owned accepted result");
        };
        let location = LibraryLocation {
            library_id,
            path: "retained.epub".parse().unwrap(),
        };
        let copied = accepted
            .candidate
            .publish(files.library(library_id).unwrap(), &location.path)
            .unwrap();
        accepted.published = Some((location.clone(), copied));
        ing.close().await;
        assert!(
            finish_accepted(&mut accepted, &config, &ing, &files, Some((&input, job)))
                .await
                .is_err()
        );
        assert!(accepted.published.is_some());
        assert!(
            files
                .library(library_id)
                .unwrap()
                .try_exists(location.path.as_path())
                .unwrap()
        );
        assert!(source.path().join("book.epub").exists());
        let resumed = ingestion_pool_for(&pool).await;
        assert!(matches!(
            finish_accepted(
                &mut accepted,
                &config,
                &resumed,
                &files,
                Some((&input, job))
            )
            .await
            .unwrap(),
            ProcessResult::Complete
        ));
        assert_eq!(
            ingestion_input::current(&resumed, input.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            InputStatus::Imported
        );
        let final_path = accepted.published.as_ref().unwrap().0.path.clone();
        assert!(matches!(
            finish_accepted(
                &mut accepted,
                &config,
                &resumed,
                &files,
                Some((&input, job))
            )
            .await
            .unwrap(),
            ProcessResult::Complete
        ));
        assert!(
            files
                .library(library_id)
                .unwrap()
                .try_exists(final_path.as_path())
                .unwrap()
        );
    }

    pub(super) fn coordinator(pool: PgPool, config: Config, files: LibraryFiles) -> Coordinator {
        Coordinator {
            config,
            pool,
            files,
            settings: crate::test_support::test_settings(),
            inputs: std::collections::HashMap::default(),
            deadlines: tokio_util::time::DelayQueue::default(),
            keys: std::collections::HashMap::default(),
            ready: std::collections::VecDeque::default(),
            ready_set: std::collections::HashSet::default(),
            observed: std::collections::HashMap::default(),
            pause: None,
            pending: None,
            lock: None,
            library_id: None,
            recovered: false,
            discovered: false,
            accepted: None,
            unresolved: std::collections::HashSet::default(),
        }
    }

    fn pending_input() -> crate::models::ingestion_input::Input {
        use crate::models::ingestion_input::{Fingerprint, Input, InputStatus};
        Input {
            id: Uuid::new_v4(),
            source_path: b"book.epub".to_vec(),
            generation: 1,
            fingerprint: sqlx::types::Json(Fingerprint {
                device: 1,
                inode: 2,
                size: 3,
                mtime_seconds: 4,
                mtime_nanoseconds: 0,
                ctime_seconds: 4,
                ctime_nanoseconds: 0,
            }),
            status: InputStatus::Pending,
            reason: None,
            work_id: None,
            retry_reset_at: chrono::Utc::now(),
            observed_at: chrono::Utc::now(),
        }
    }

    fn no_retry(id: Uuid) -> crate::models::ingestion_input::RetryState {
        crate::models::ingestion_input::RetryState {
            id,
            count: 0,
            failed_at: None,
            needs_change: false,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn capability_ingestion_coordinator_readiness_coalesces_and_other_ready_input_proceeds() {
        use futures::StreamExt;
        let state = crate::test_support::test_state();
        let mut owner = coordinator(state.pool, state.config, state.library_files);
        let input = pending_input();
        let id = input.id;
        owner.observed.insert(
            input.source_path.clone(),
            (input.fingerprint.0.clone(), tokio::time::Instant::now()),
        );
        assert_eq!(owner.schedule(input.clone(), &no_retry(id)), 1);
        tokio::time::advance(std::time::Duration::from_secs(9)).await;
        assert_eq!(owner.schedule(input, &no_retry(id)), 1);
        assert_eq!(owner.keys.len(), 1);
        let mut ready = pending_input();
        ready.source_path = b"ready.epub".to_vec();
        owner.observed.insert(
            ready.source_path.clone(),
            (
                ready.fingerprint.0.clone(),
                tokio::time::Instant::now() - READINESS,
            ),
        );
        let ready_id = ready.id;
        assert_eq!(owner.schedule(ready, &no_retry(ready_id)), 0);
        assert_eq!(owner.ready.pop_front(), Some(ready_id));
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        let expired = owner.deadlines.next().await.unwrap();
        assert_eq!(expired.into_inner(), id);
        owner.keys.remove(&id);
        assert!(owner.deadlines.is_empty());
        owner.forget(id);
        let replacement = pending_input();
        owner.observed.insert(
            replacement.source_path.clone(),
            (
                replacement.fingerprint.0.clone(),
                tokio::time::Instant::now(),
            ),
        );
        assert_eq!(
            owner.schedule(replacement.clone(), &no_retry(replacement.id)),
            1
        );
        assert_eq!(owner.keys.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn capability_ingestion_coordinator_five_retries_then_sixth_failure_exhausts() {
        use crate::models::ingestion_input::{InputStatus, RetryState};
        let state = crate::test_support::test_state();
        let mut owner = coordinator(state.pool, state.config, state.library_files);
        assert_eq!(RETRIES, [300, 1800, 7200, 28800, 86400]);
        for count in 1..=6 {
            let mut input = pending_input();
            input.source_path = format!("retry-{count}.epub").into_bytes();
            input.status = InputStatus::OperationalFailure;
            owner.observed.insert(
                input.source_path.clone(),
                (
                    input.fingerprint.0.clone(),
                    tokio::time::Instant::now() - READINESS,
                ),
            );
            let id = input.id;
            let retry = RetryState {
                id,
                count,
                failed_at: Some(chrono::Utc::now()),
                needs_change: false,
            };
            assert_eq!(owner.schedule(input, &retry), if count < 6 { 1 } else { 2 });
            if count < 6 {
                let key = owner.keys.get(&id).unwrap();
                let remaining = owner
                    .deadlines
                    .deadline(key)
                    .duration_since(tokio::time::Instant::now());
                assert!(
                    remaining
                        .as_secs()
                        .abs_diff(RETRIES[usize::try_from(count - 1).unwrap()])
                        <= 1
                );
            } else {
                assert!(!owner.keys.contains_key(&id));
                assert!(!owner.ready.contains(&id));
            }
        }
        let mut input = pending_input();
        input.source_path = b"needs-change.epub".to_vec();
        input.status = InputStatus::OperationalFailure;
        let mut retry = no_retry(input.id);
        retry.needs_change = true;
        assert_eq!(owner.schedule(input.clone(), &retry), 2);
        assert!(!owner.keys.contains_key(&input.id));
        input.status = InputStatus::Pending;
        input.retry_reset_at += chrono::Duration::seconds(1);
        assert_eq!(owner.schedule(input.clone(), &no_retry(input.id)), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn capability_ingestion_phase_protected_operations_survive_idle_and_reset_on_transition()
    {
        use futures::FutureExt;
        for phase in [copier::Phase::Validation, copier::Phase::Publication] {
            let shutdown = CancellationToken::new();
            let progress = copier::Progress::new(&shutdown);
            let now = tokio::time::Instant::now();
            let mut active = Active {
                future: std::future::pending().boxed(),
                progress: progress.clone(),
                count: 0,
                transitions: 0,
                last_progress: now,
                next_warning: now + STALL,
                recommit: false,
            };
            tokio::time::advance(IDLE.checked_sub(std::time::Duration::from_secs(1)).unwrap())
                .await;
            progress.enter(phase).unwrap();
            active.tick();
            tokio::time::advance(IDLE * 2).await;
            active.tick();
            assert!(!progress.cancel.is_cancelled());
            tokio::time::advance(STALL).await;
            active.tick();
            let warning = active.next_warning;
            tokio::time::advance(STALL).await;
            active.tick();
            assert_eq!(active.next_warning, warning + STALL);
            progress.enter(copier::Phase::Streaming).unwrap();
            active.tick();
            tokio::time::advance(IDLE.checked_sub(std::time::Duration::from_secs(1)).unwrap())
                .await;
            active.tick();
            assert!(!progress.cancel.is_cancelled());
            shutdown.cancel();
            assert!(matches!(
                progress.check(),
                Err(copier::CopyError::Cancelled)
            ));
            let transitions = progress.transitions();
            assert!(progress.enter(phase).is_err());
            assert_eq!(progress.transitions(), transitions);
            assert!(progress.cancel.is_cancelled());
            assert!(active.future.now_or_never().is_none());
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_phase_cancellation_after_publication_preserves_source(
        pool: PgPool,
    ) {
        let ing = ingestion_pool_for(&pool).await;
        let (source, _library, config) = scan_env();
        let library_id = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let mut owner = coordinator(ing.clone(), config, files);
        std::fs::write(source.path().join("book.epub"), make_minimal_epub()).unwrap();
        let (input, job, mut accepted, publication) =
            published_attempt(&mut owner, "book.epub").await;
        accepted.candidate.progress().cancel.cancel();
        let result = finish_accepted(
            &mut accepted,
            &owner.config,
            &ing,
            &owner.files,
            Some((&input, job)),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            ProcessResult::Operational(
                crate::models::ingestion_input::AttemptOutcome::TransientInput,
                _
            )
        ));
        assert!(
            !owner
                .files
                .library(library_id)
                .unwrap()
                .try_exists(&publication.path)
                .unwrap()
        );
        assert!(source.path().join("book.epub").exists());
        assert!(
            crate::models::ingestion_input::publication_page(&ing, None)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn capability_ingestion_phase_idle_cancellation_stall_warnings_and_no_total_deadline() {
        use futures::FutureExt;
        let progress = copier::Progress::new(&CancellationToken::new());
        let now = tokio::time::Instant::now();
        let mut active = Active {
            future: std::future::pending().boxed(),
            progress: progress.clone(),
            count: 0,
            transitions: 0,
            last_progress: now,
            next_warning: now + STALL,
            recommit: false,
        };
        tokio::time::advance(IDLE.checked_sub(std::time::Duration::from_secs(1)).unwrap()).await;
        active.tick();
        assert!(!progress.cancel.is_cancelled());
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        active.tick();
        assert!(progress.cancel.is_cancelled());
        assert!(active.future.now_or_never().is_none());
        active.future = std::future::pending().boxed();
        tokio::time::advance(STALL.checked_sub(IDLE).unwrap()).await;
        active.tick();
        assert_eq!(active.next_warning, now + STALL * 2);
        tokio::time::advance(STALL).await;
        active.tick();
        assert_eq!(active.next_warning, now + STALL * 3);
        let source = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let source =
            cap_std::fs::Dir::open_ambient_dir(source.path(), cap_std::ambient_authority())
                .unwrap();
        let library =
            cap_std::fs::Dir::open_ambient_dir(library.path(), cap_std::ambient_authority())
                .unwrap();
        source.write("book.epub", b"stable bytes").unwrap();
        let path =
            crate::models::ingestion_input::InputPath::from_path(Path::new("book.epub")).unwrap();
        let fingerprint = crate::models::ingestion_input::Fingerprint::from_metadata(
            &copier::input_metadata(&source, &path).unwrap(),
        );
        active.progress = copier::Progress::new(&CancellationToken::new());
        active.last_progress = tokio::time::Instant::now();
        active.count = 0;
        for _ in 0..4 {
            tokio::time::advance(std::time::Duration::from_secs(100)).await;
            copier::acquire_controlled(
                &source,
                &path,
                &library,
                &"book.epub".parse().unwrap(),
                &fingerprint,
                active.progress.clone(),
            )
            .unwrap()
            .close()
            .unwrap();
            active.tick();
            assert!(!active.progress.cancel.is_cancelled());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn capability_ingestion_coordinator_shared_probe_schedule_and_single_pause() {
        let state = crate::test_support::test_state();
        let mut owner = coordinator(state.pool, state.config, state.library_files);
        owner.pause(&"database unavailable");
        let first = owner.pause.as_ref().unwrap().next;
        owner.pause(&"root unavailable");
        assert_eq!(owner.pause.as_ref().unwrap().next, first);
        for seconds in [30, 60, 120, 300, 300] {
            let pause = owner.pause.as_mut().unwrap();
            assert_eq!(
                pause
                    .next
                    .duration_since(tokio::time::Instant::now())
                    .as_secs(),
                seconds
            );
            tokio::time::advance(std::time::Duration::from_secs(seconds)).await;
            pause.failed();
        }
    }

    #[test]
    fn capability_ingestion_coordinator_root_faults_and_needs_change_classes() {
        use crate::models::ingestion_input::AttemptOutcome;
        for errno in [
            rustix::io::Errno::IO,
            rustix::io::Errno::NOTCONN,
            rustix::io::Errno::STALE,
            rustix::io::Errno::HOSTDOWN,
            rustix::io::Errno::NOSPC,
        ] {
            let error =
                copier::CopyError::Io(std::io::Error::from_raw_os_error(errno.raw_os_error()));
            assert!(matches!(
                classify_acquisition_error(&error, false, false, true),
                FailureDisposition::Failure(AttemptOutcome::SharedDependency, _)
            ));
        }
        for errno in [
            rustix::io::Errno::ACCESS,
            rustix::io::Errno::PERM,
            rustix::io::Errno::NAMETOOLONG,
            rustix::io::Errno::LOOP,
        ] {
            let error =
                copier::CopyError::Io(std::io::Error::from_raw_os_error(errno.raw_os_error()));
            assert!(matches!(
                classify_acquisition_error(&error, false, true, true),
                FailureDisposition::Failure(AttemptOutcome::NeedsChange, _)
            ));
        }
        assert!(matches!(
            classify_acquisition_error(&copier::CopyError::NonRegular, false, true, true),
            FailureDisposition::Failure(AttemptOutcome::NeedsChange, _)
        ));
        let error = copier::CopyError::DestinationIo(std::io::Error::from_raw_os_error(
            rustix::io::Errno::NOSPC.raw_os_error(),
        ));
        assert!(matches!(
            classify_acquisition_error(&error, false, true, false),
            FailureDisposition::Failure(AttemptOutcome::SharedDependency, _)
        ));
    }

    #[test]
    fn capability_ingestion_acquisition_rejected_candidate_never_publishes() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            cap_std::fs::Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority())
                .unwrap();
        let library =
            cap_std::fs::Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority())
                .unwrap();
        source.write("bad.epub", b"corrupt EPUB").unwrap();
        let path = InputPath::from_path(Path::new("bad.epub")).unwrap();
        let observed = Fingerprint::from_metadata(
            &copier::open_input(&source, &path)
                .unwrap()
                .metadata()
                .unwrap(),
        );
        let candidate = copier::acquire(
            &source,
            &path,
            &library,
            &"bad.epub".parse().unwrap(),
            &observed,
        )
        .unwrap();
        assert_eq!(
            candidate.validate().unwrap().report.outcome,
            ValidationOutcome::Quarantined
        );
        assert!(!library.try_exists("bad.epub").unwrap());
        candidate.close().unwrap();
        assert_eq!(source.read("bad.epub").unwrap(), b"corrupt EPUB");
        assert_eq!(library.entries().unwrap().count(), 0);
    }

    #[test]
    fn capability_ingestion_acquisition_hardlink_repair_owns_independent_bytes() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            cap_std::fs::Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority())
                .unwrap();
        let library =
            cap_std::fs::Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority())
                .unwrap();
        let original = make_repaired_epub();
        source.write("repair.epub", &original).unwrap();
        source
            .hard_link("repair.epub", &source, "torrent.epub")
            .unwrap();
        let path = InputPath::from_path(Path::new("repair.epub")).unwrap();
        let observed = Fingerprint::from_metadata(
            &copier::open_input(&source, &path)
                .unwrap()
                .metadata()
                .unwrap(),
        );
        let relative = "Author/repair.epub".parse().unwrap();
        let candidate = copier::acquire(&source, &path, &library, &relative, &observed).unwrap();
        assert_eq!(
            candidate.validate().unwrap().report.outcome,
            ValidationOutcome::Repaired
        );
        let copied = candidate.publish(&library, &relative).unwrap();
        assert_ne!(library.read(relative.as_path()).unwrap(), original);
        assert_eq!(source.read("repair.epub").unwrap(), original);
        assert_eq!(source.read("torrent.epub").unwrap(), original);
        assert_ne!(copied.identity, (observed.device, observed.inode));
        candidate.close().unwrap();
    }

    #[test]
    fn capability_ingestion_acquisition_source_change_and_no_overwrite_preserve_bytes() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            cap_std::fs::Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority())
                .unwrap();
        let library =
            cap_std::fs::Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority())
                .unwrap();
        source.write("book.epub", make_minimal_epub()).unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let observed = Fingerprint::from_metadata(
            &copier::open_input(&source, &path)
                .unwrap()
                .metadata()
                .unwrap(),
        );
        let relative = "book.epub".parse().unwrap();
        let candidate = copier::acquire(&source, &path, &library, &relative, &observed).unwrap();
        library.write("book.epub", b"foreign owner").unwrap();
        assert!(candidate.publish(&library, &relative).is_err());
        assert_eq!(library.read("book.epub").unwrap(), b"foreign owner");
        source.write("book.epub", b"new source generation").unwrap();
        assert!(matches!(
            candidate.verify_source(&source, &path),
            Err(copier::CopyError::Changed)
        ));
        candidate.close().unwrap();
        assert_eq!(source.read("book.epub").unwrap(), b"new source generation");
    }

    #[test]
    fn capability_ingestion_coordinator_hash_mismatch_rechecks_source_before_failure() {
        use crate::models::ingestion_input::{AttemptOutcome, Fingerprint, InputPath};
        let root = tempfile::tempdir().unwrap();
        let dir =
            cap_std::fs::Dir::open_ambient_dir(root.path(), cap_std::ambient_authority()).unwrap();
        dir.write("source.epub", b"first").unwrap();
        let path = InputPath::from_path(Path::new("source.epub")).unwrap();
        let original = Fingerprint::from_metadata(
            &copier::open_input(&dir, &path).unwrap().metadata().unwrap(),
        );
        let error = copier::CopyError::HashMismatch {
            source_hash: "first".into(),
            dest_hash: "second".into(),
        };
        let unchanged = Fingerprint::from_metadata(
            &copier::open_input(&dir, &path).unwrap().metadata().unwrap(),
        );
        assert!(matches!(
            classify_acquisition_error(&error, unchanged != original, true, true),
            FailureDisposition::Failure(AttemptOutcome::TransientInput, _)
        ));
        dir.write("source.epub", b"changed and longer").unwrap();
        let changed = Fingerprint::from_metadata(
            &copier::open_input(&dir, &path).unwrap().metadata().unwrap(),
        );
        assert_eq!(
            classify_acquisition_error(&error, changed != original, true, true),
            FailureDisposition::Changed
        );
        assert_eq!(dir.read("source.epub").unwrap(), b"changed and longer");
    }

    #[test]
    fn capability_ingestion_coordinator_unlisted_io_is_transient_with_error_kind() {
        use crate::models::ingestion_input::AttemptOutcome;
        for kind in [
            std::io::ErrorKind::WriteZero,
            std::io::ErrorKind::UnexpectedEof,
            std::io::ErrorKind::Other,
        ] {
            let error = copier::CopyError::Io(std::io::Error::new(kind, "input operation failed"));
            match classify_acquisition_error(&error, false, true, true) {
                FailureDisposition::Failure(AttemptOutcome::TransientInput, reason) => {
                    assert!(reason.contains(&format!("{kind:?}")));
                }
                disposition => panic!("incorrect classification: {disposition:?}"),
            }
            assert!(matches!(
                classify_acquisition_error(&error, false, false, true),
                FailureDisposition::Failure(AttemptOutcome::SharedDependency, _)
            ));
        }
    }

    struct AttemptCounts {
        processed: usize,
        skipped: usize,
        failed: usize,
    }

    async fn drive_owner(config: &Config, pool: &PgPool) -> anyhow::Result<AttemptCounts> {
        use futures::StreamExt;
        let id = crate::models::storage_library::default_library_id(pool).await?;
        let files =
            LibraryFiles::open([(id, config.library_path.clone())], &config.ingestion_path)?;
        let mut owner = coordinator(pool.clone(), config.clone(), files);
        {
            let mut settings = owner.settings.write().await;
            settings.ingestion.accepted_formats = config
                .accepted_formats
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            settings.ingestion.cleanup_imported = config.cleanup_imported;
            settings.ingestion.cleanup_duplicates = config.cleanup_duplicates;
        }
        owner.probe().await?;
        owner.discover(false).await?;
        let count = owner
            .inputs
            .values()
            .filter(|input| input.status == crate::models::ingestion_input::InputStatus::Pending)
            .count();
        let mut result = AttemptCounts {
            processed: 0,
            skipped: 0,
            failed: 0,
        };
        let cancel = CancellationToken::new();
        for _ in 0..count {
            while owner.ready.is_empty() {
                let expired = owner
                    .deadlines
                    .next()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("missing readiness deadline"))?;
                let id = expired.into_inner();
                owner.keys.remove(&id);
                if owner.ready_set.insert(id) {
                    owner.ready.push_back(id);
                }
            }
            let mut active = owner
                .start_attempt(&cancel)
                .await?
                .ok_or_else(|| anyhow::anyhow!("attempt unavailable"))?;
            let pending = (&mut active.future).await;
            match &pending.result {
                ProcessResult::Complete => result.processed += 1,
                ProcessResult::Skipped(_) => result.skipped += 1,
                ProcessResult::Failed(_) | ProcessResult::Operational(_, _) => result.failed += 1,
                ProcessResult::Changed => {}
                ProcessResult::Accepted(_) => anyhow::bail!("unresolved accepted result"),
            }
            owner.pending = Some(pending);
            owner.finalise().await?;
        }
        Ok(result)
    }

    use crate::test_support::db::ingestion_pool_for;

    #[test]
    fn warms_only_when_a_cover_could_be_served() {
        assert!(should_warm_cover(ManifestationFormat::Epub, Some(true)));
        assert!(should_warm_cover(ManifestationFormat::Epub, None));
        assert!(!should_warm_cover(ManifestationFormat::Epub, Some(false)));
        assert!(!should_warm_cover(ManifestationFormat::Pdf, None));
        assert!(!should_warm_cover(ManifestationFormat::Pdf, Some(true)));
    }

    fn test_config_for(ingestion: &str, library: &str) -> Config {
        Config {
            port: 3000,
            database_url: String::new(),
            library_path: library.parse().unwrap(),
            ingestion_path: ingestion.parse().unwrap(),
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            local_auth_enabled: true,
            resource_server_issuer: String::new(),
            resource_server_audience: String::new(),
            resource_server_jwks_url: String::new(),
            resource_server_require_at_jwt: false,
            login_rate_per_min: 10,
            login_throttle_base_secs: 2,
            login_throttle_cap_secs: 900,
            password_min_length: 8,
            password_max_length: 256,
            password_min_zxcvbn_score: 2,
            password_breach_check_enabled: true,
            self_registration_enabled: false,
            recovery_pin_ttl_secs: 900,
            recovery_pin_dir: "./reverie-recovery".into(),
            trusted_client_ip_header: None,
            migration_database_url: None,
            auto_migrate: false,
            ingestion_database_url: String::new(),
            accepted_formats: vec![ManifestationFormat::Epub],
            cleanup_imported: false,
            cleanup_duplicates: false,
            enrichment: crate::config::EnrichmentConfig {
                enabled: false,
                concurrency: 1,
                poll_idle_secs: 30,
                fetch_budget_secs: 15,
                http_timeout_secs: 10,
                max_attempts: 3,
                cache_ttl_hit_days: 1,
                cache_ttl_miss_days: 1,
                cache_ttl_error_mins: 1,
            },
            cover: crate::config::CoverConfig {
                max_bytes: 10_485_760,
                download_timeout_secs: 30,
                min_long_edge_px: 1000,
                redirect_limit: 3,
            },
            writeback: crate::config::WritebackConfig {
                enabled: false,
                concurrency: 1,
                poll_idle_secs: 5,
                max_attempts: 3,
            },
            opds: crate::config::OpdsConfig {
                enabled: false,
                page_size: 50,
                realm: "Reverie OPDS".into(),
                public_url: None,
            },
            security: crate::config::SecurityConfig {
                behind_https: false,
                hsts_include_subdomains: false,
                hsts_preload: false,
                csp_report_endpoint: None,
                frontend_dist_path: None,
                csp_html_header: None,
                csp_api_header: None,
            },
            googlebooks_api_key: None,
            hardcover_api_token: None,
            operator_contact: None,
            ingestion_dsn_defaulted: false,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_empty_dir_returns_zero(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_ingestion, _library, config) = scan_env();
        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_records_pdf_as_not_accepted(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();
        let source = ingestion.path().join("Tolkien - The Hobbit.pdf");
        std::fs::write(&source, b"PDF bytes").unwrap();
        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!((result.processed, result.failed, result.skipped), (0, 0, 0));
        let input = crate::models::ingestion_input::current_page(&pool, None)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            input.status,
            crate::models::ingestion_input::InputStatus::NotAccepted
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"PDF bytes");
        assert!(!library.path().join("Tolkien/The Hobbit.pdf").exists());
    }

    fn scan_env() -> (tempfile::TempDir, tempfile::TempDir, Config) {
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
        );
        (ingestion, library, config)
    }

    fn publish_fixture(
        source: &Path,
        root: &cap_std::fs::Dir,
        relative: &RelativeFilePath,
    ) -> Result<copier::CopyResult, copier::CopyError> {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let directory = cap_std::fs::Dir::open_ambient_dir(
            source.parent().unwrap(),
            cap_std::ambient_authority(),
        )?;
        let path = InputPath::from_path(Path::new(source.file_name().unwrap()))?;
        let fingerprint = Fingerprint::from_metadata(&copier::input_metadata(&directory, &path)?);
        let candidate = copier::acquire(&directory, &path, root, relative, &fingerprint)?;
        let prepared = candidate.prepare(
            root,
            relative,
            &candidate.ingestion_hash,
            fingerprint.size,
            false,
        )?;
        let result = prepared.publish(root, relative)?;
        candidate.close()?;
        Ok(result)
    }

    async fn claim_copy_fixture(
        pool: &PgPool,
        path: &str,
    ) -> (
        tempfile::TempDir,
        LibraryFiles,
        LibraryLocation,
        copier::CopyResult,
    ) {
        let library_id = crate::models::storage_library::default_library_id(pool)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, library_id);
        let source = dir.path().join("input.pdf");
        std::fs::write(&source, b"candidate bytes").unwrap();
        let location = LibraryLocation {
            library_id,
            path: path.parse().unwrap(),
        };
        let copied =
            publish_fixture(&source, files.library(library_id).unwrap(), &location.path).unwrap();
        (dir, files, location, copied)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_acquisition_import_commits_attempt_and_input_atomically(
        pool: PgPool,
    ) {
        use crate::models::ingestion_input::{self, Fingerprint, InputPath, InputStatus};
        let ing = ingestion_pool_for(&pool).await;
        let (dir, _files, location, copied) = claim_copy_fixture(&pool, "atomic.epub").await;
        let fingerprint =
            Fingerprint::from_metadata(&std::fs::metadata(dir.path().join("input.pdf")).unwrap());
        let input = ingestion_input::observe(
            &ing,
            &[InputPath::from_path(Path::new("input.epub")).unwrap()],
            &[fingerprint],
        )
        .await
        .unwrap()
        .remove(0);
        let job = ingestion_input::begin_attempt(&ing, &input, Uuid::new_v4())
            .await
            .unwrap();
        let vars = path_template::heuristic_vars_from_filename("Atomic.epub");
        let meta = || ManifestationMeta {
            format: ManifestationFormat::Epub,
            validation_status: ValidationStatus::Clean,
            accessibility_metadata: &None,
            has_embedded_cover: Some(false),
            current_hash: &copied.sha256,
            current_size: copied.file_size,
        };
        assert!(
            commit_ingest_outcome(
                &ing,
                None,
                &vars,
                &location,
                &copied,
                meta(),
                Some((&input, Uuid::new_v4()))
            )
            .await
            .is_err()
        );
        assert_eq!(
            ingestion_input::current_page(&ing, None).await.unwrap()[0].status,
            InputStatus::Processing
        );
        let mut tx = ing.begin().await.unwrap();
        assert!(
            library_path_claim::owner(&mut tx, &location)
                .await
                .unwrap()
                .is_none()
        );
        tx.rollback().await.unwrap();
        let (work, manifestation) = commit_ingest_outcome(
            &ing,
            None,
            &vars,
            &location,
            &copied,
            meta(),
            Some((&input, job)),
        )
        .await
        .unwrap();
        let imported = ingestion_input::current_page(&ing, None)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(imported.status, InputStatus::Imported);
        assert_eq!(imported.work_id, Some(work));
        let mut tx = ing.begin().await.unwrap();
        assert_eq!(
            library_path_claim::owner(&mut tx, &location).await.unwrap(),
            Some(manifestation)
        );
        tx.rollback().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_ingestion_manifestation_commit_and_competing_owner(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let (_dir, files, location, copied) = claim_copy_fixture(&pool, "atomic.pdf").await;
        let vars = path_template::heuristic_vars_from_filename("Atomic.pdf");
        let meta = || ManifestationMeta {
            format: ManifestationFormat::Pdf,
            validation_status: ValidationStatus::Pending,
            accessibility_metadata: &None,
            has_embedded_cover: None,
            current_hash: &copied.sha256,
            current_size: copied.file_size,
        };
        let (_, id) = commit_ingest(&ing, &None, &vars, &location, &copied, meta())
            .await
            .unwrap();
        assert_eq!(sqlx::query_scalar!("SELECT manifestation_id FROM library_path_claims WHERE library_id = $1 AND path = $2", location.library_id.as_uuid(), location.path.as_str()).fetch_one(&pool).await.unwrap(), id);
        assert_eq!(
            sqlx::query_scalar!("SELECT file_path FROM manifestations WHERE id = $1", id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "atomic.pdf"
        );
        let competing = LibraryLocation {
            library_id: location.library_id,
            path: "competing.pdf".parse().unwrap(),
        };
        let mut tx = ing.begin().await.unwrap();
        assert!(
            library_path_claim::reserve(&mut tx, &competing, id)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        assert!(
            commit_ingest(&ing, &None, &vars, &competing, &copied, meta())
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM manifestations WHERE library_id = $1 AND file_path = $2",
                competing.library_id.as_uuid(),
                competing.path.as_str()
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some(0)
        );
        assert!(
            cleanup_candidate(&ing, &files, &location, copied.identity)
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_ingestion_unsupported_format_cleans_only_its_candidate(pool: PgPool) {
        let library_id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let ing = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();
        let files = crate::test_support::test_library_files_at(&config.library_path, library_id);
        let source = ingestion.path().join("Author - Unsupported.bin");
        std::fs::write(&source, b"unsupported bytes").unwrap();
        let result = process_file(
            &source,
            &config,
            &ing,
            &files,
            library_id,
            None,
            copier::Progress::new(&CancellationToken::new()),
        )
        .await;
        assert!(
            matches!(result, ProcessResult::Failed(reason) if reason.contains("unsupported format"))
        );
        assert!(!library.path().join("Author/Unsupported.bin").exists());
        assert_eq!(std::fs::read(source).unwrap(), b"unsupported bytes");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_ingestion_cleanup_preserves_committed_owner_after_lost_ack(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let (_dir, files, location, copied) =
            claim_copy_fixture(&pool, "fixtures/admin-test-ack.epub").await;
        crate::test_support::db::insert_work_and_manifestation(&ing, "ack").await;
        let error = cleanup_candidate(&ing, &files, &location, copied.identity)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("committed or claimed"));
        let ordinary = crate::test_support::db::app_pool_for(&pool).await;
        let error = cleanup_candidate(&ordinary, &files, &location, copied.identity)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("ownership evidence is unavailable")
        );
        assert_eq!(
            files
                .library(location.library_id)
                .unwrap()
                .read(location.path.as_path())
                .unwrap(),
            b"candidate bytes"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_ingestion_cleanup_definite_failure_and_changed_identity(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let (dir, files, location, copied) = claim_copy_fixture(&pool, "candidate.pdf").await;
        let (_, owner) =
            crate::test_support::db::insert_work_and_manifestation(&ing, "rollback").await;
        let mut tx = ing.begin().await.unwrap();
        assert!(
            library_path_claim::reserve(&mut tx, &location, owner)
                .await
                .unwrap()
        );
        tx.rollback().await.unwrap();
        cleanup_candidate(&ing, &files, &location, copied.identity)
            .await
            .unwrap();
        assert!(!dir.path().join(location.path.as_path()).exists());
        let (_changed_dir, files, location, copied) =
            claim_copy_fixture(&pool, "changed.pdf").await;
        let root = files.library(location.library_id).unwrap();
        root.rename(location.path.as_path(), root, "retained-original.pdf")
            .unwrap();
        root.write(location.path.as_path(), b"foreign bytes")
            .unwrap();
        assert!(
            cleanup_candidate(&ing, &files, &location, copied.identity)
                .await
                .unwrap_err()
                .to_string()
                .contains("identity changed")
        );
        assert_eq!(
            root.read(location.path.as_path()).unwrap(),
            b"foreign bytes"
        );
        ing.close().await;
        assert!(
            cleanup_candidate(&ing, &files, &location, copied.identity)
                .await
                .is_err()
        );
        assert_eq!(
            root.read(location.path.as_path()).unwrap(),
            b"foreign bytes"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_ingestion_cleanup_cancellation_keeps_exclusion_until_mutation_finishes(
        pool: PgPool,
    ) {
        let ing = ingestion_pool_for(&pool).await;
        let (_dir, files, location, copied) = claim_copy_fixture(&pool, "cancelled.pdf").await;
        let (started, reached) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::sync_channel(1);
        let task_pool = ing.clone();
        let task_files = files.clone();
        let task_location = location.clone();
        let task = tokio::spawn(async move {
            cleanup_candidate_with(
                &task_pool,
                &task_files,
                &task_location,
                copied.identity,
                move |parent, name| {
                    started.send(()).unwrap();
                    released
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    parent.remove_file(name)
                },
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), reached)
            .await
            .unwrap()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let next_pool = ing.clone();
        let next_location = location.clone();
        let mut next = tokio::spawn(async move {
            let mut tx = next_pool.begin().await.unwrap();
            library_path_claim::exclude(&mut tx, &next_location)
                .await
                .unwrap();
            tx.rollback().await.unwrap();
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut next)
                .await
                .is_err()
        );
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), next)
            .await
            .unwrap()
            .unwrap();
        assert!(
            files
                .library(location.library_id)
                .unwrap()
                .symlink_metadata(location.path.as_path())
                .is_err()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_ingestion_selection_skips_absent_foreign_claim_and_copy_refuses_race(
        pool: PgPool,
    ) {
        let ing = ingestion_pool_for(&pool).await;
        let (dir, files, location, copied) = claim_copy_fixture(&pool, "candidate.pdf").await;
        let (_, owner) =
            crate::test_support::db::insert_work_and_manifestation(&ing, "reserved").await;
        let mut tx = ing.begin().await.unwrap();
        let reserved = LibraryLocation {
            library_id: location.library_id,
            path: "absent.pdf".parse().unwrap(),
        };
        assert!(
            library_path_claim::reserve(&mut tx, &reserved, owner)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        let (selected, tx) =
            select_ingestion_path(&ing, &files, location.library_id, &reserved.path)
                .await
                .unwrap();
        assert_eq!(selected.path.as_str(), "absent (2).pdf");
        tx.rollback().await.unwrap();
        assert!(
            publish_fixture(
                &dir.path().join("input.pdf"),
                files.library(location.library_id).unwrap(),
                &location.path,
            )
            .is_err()
        );
        assert_eq!(
            files
                .library(location.library_id)
                .unwrap()
                .read(location.path.as_path())
                .unwrap(),
            b"candidate bytes"
        );
        let (_, tx) = select_ingestion_path(
            &ing,
            &files,
            location.library_id,
            &"metadata.pdf".parse().unwrap(),
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        files
            .library(location.library_id)
            .unwrap()
            .write("metadata.pdf", b"foreign destination")
            .unwrap();
        assert!(
            path_rename::move_existing(
                files.library(location.library_id).unwrap(),
                &location.path,
                &"metadata.pdf".parse().unwrap(),
                &copied.sha256
            )
            .is_err()
        );
        assert_eq!(
            files
                .library(location.library_id)
                .unwrap()
                .read("metadata.pdf")
                .unwrap(),
            b"foreign destination"
        );
    }

    const CONTAINER_XML: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    /// Assemble an EPUB ZIP in memory: stored `mimetype`, an optional
    /// `META-INF/container.xml`, and the given OPF at `OEBPS/content.opf`.
    fn build_epub(opf: &[u8], container_xml: Option<&[u8]>) -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::{ExtendedFileOptions, FileOptions};

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);

        // mimetype must be first and stored (not deflated) per EPUB spec
        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();

        let default: FileOptions<ExtendedFileOptions> = FileOptions::default();

        if let Some(container) = container_xml {
            w.start_file("META-INF/container.xml", default.clone())
                .unwrap();
            w.write_all(container).unwrap();
        }

        w.start_file("OEBPS/content.opf", default).unwrap();
        w.write_all(opf).unwrap();

        w.finish().unwrap().into_inner()
    }

    /// Build a minimal valid EPUB ZIP in memory. All layers pass cleanly:
    /// valid ZIP, valid container, valid OPF with empty manifest and spine,
    /// no XHTML to check, no cover declared.
    fn make_minimal_epub() -> Vec<u8> {
        build_epub(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest/>
  <spine/>
</package>"#,
            Some(CONTAINER_XML),
        )
    }

    /// Build an EPUB declaring a decodable cover image (`properties="cover-image"`)
    /// so ingestion's Layer 5 cover check finds it and sets
    /// `has_embedded_cover=true` on the manifestation row.
    fn make_epub_with_cover() -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::{ExtendedFileOptions, FileOptions};

        let mut png_bytes: Vec<u8> = Vec::new();
        let img = image::DynamicImage::new_rgb8(1, 1);
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();

        let default: FileOptions<ExtendedFileOptions> = FileOptions::default();
        w.start_file("META-INF/container.xml", default.clone())
            .unwrap();
        w.write_all(CONTAINER_XML).unwrap();

        w.start_file("OEBPS/content.opf", default.clone()).unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest>
    <item id="cover-image" href="cover.png" media-type="image/png" properties="cover-image"/>
  </manifest>
  <spine/>
</package>"#,
        )
        .unwrap();

        w.start_file("OEBPS/cover.png", default).unwrap();
        w.write_all(&png_bytes).unwrap();

        w.finish().unwrap().into_inner()
    }

    /// Build an EPUB with Dublin Core metadata for integration testing.
    fn make_metadata_epub() -> Vec<u8> {
        build_epub(
            br##"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" version="3.0">
  <metadata>
    <dc:title id="t1">The Integration Test</dc:title>
    <dc:title id="t2">A Subtitle For Testing</dc:title>
    <meta refines="#t2" property="title-type">subtitle</meta>
    <dc:creator opf:role="aut">Test McAuthor</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier>urn:isbn:9780306406157</dc:identifier>
    <dc:publisher>Test Press</dc:publisher>
    <dc:description>A book for testing metadata extraction</dc:description>
    <meta property="schema:numberOfPages">327</meta>
    <meta name="calibre:series" content="Test Series"/>
    <meta name="calibre:series_index" content="1"/>
  </metadata>
  <manifest/>
  <spine/>
</package>"##,
            Some(CONTAINER_XML),
        )
    }

    /// Build an EPUB the validator rates `Repaired`: a valid `.opf` at a safe
    /// path but no `META-INF/container.xml`, so the container layer regenerates
    /// it (a `Repaired`-severity fix) and repacks the file.
    fn make_repaired_epub() -> Vec<u8> {
        build_epub(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest/>
  <spine/>
</package>"#,
            None,
        )
    }

    /// Build an EPUB the validator rates `Degraded`: a complete, valid container
    /// whose OPF declares a cover image, but the cover file is absent from the
    /// ZIP — the cover layer records a `MissingCover` (`Degraded`) issue with no
    /// repairable issue present.
    fn make_degraded_epub() -> Vec<u8> {
        // Manifest declares cover.jpg but the entry is never written to the ZIP.
        build_epub(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest>
    <item id="cover-image" href="cover.jpg" media-type="image/jpeg"/>
  </manifest>
  <spine/>
</package>"#,
            Some(CONTAINER_XML),
        )
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_extracts_metadata_from_epub(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        // Use a filename that differs from the OPF metadata to test rename
        let source = ingestion.path().join("Unknown - somefile.epub");
        std::fs::write(&source, make_metadata_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1);
        assert_eq!(result.failed, 0);

        // File should be renamed to metadata-based path: "McAuthor, Test/The Integration Test.epub"
        let dest = library
            .path()
            .join("McAuthor, Test/The Integration Test.epub");
        assert!(
            dest.exists(),
            "expected metadata-renamed file at {}",
            dest.display()
        );

        // Verify work title
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let title = sqlx::query_scalar!(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(title, "The Integration Test");

        // Verify author was created and linked
        let author_name = sqlx::query_scalar!(
            "SELECT a.name FROM authors a \
             JOIN work_authors wa ON wa.author_id = a.id \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_name, "Test McAuthor");

        // Verify ISBN was populated on the manifestation
        let isbn = sqlx::query_scalar!(
            "SELECT isbn_13 FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(isbn.as_deref(), Some("9780306406157"));

        // Verify subtitle landed on the work with its pointer set.
        let (subtitle, subtitle_version_id) = sqlx::query!(
            "SELECT w.subtitle, w.subtitle_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .map(|r| (r.subtitle, r.subtitle_version_id))
        .unwrap();
        assert_eq!(subtitle.as_deref(), Some("A Subtitle For Testing"));
        assert!(subtitle_version_id.is_some());

        // Verify page count landed on the manifestation with its pointer set.
        let (pages, pages_version_id) = sqlx::query!(
            "SELECT pages, pages_version_id FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .map(|r| (r.pages, r.pages_version_id))
        .unwrap();
        assert_eq!(pages, Some(327));
        assert!(pages_version_id.is_some());

        // Verify metadata_versions drafts were created
        let draft_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions mv \
             JOIN manifestations m ON m.id = mv.manifestation_id \
             WHERE m.file_path = $1 AND mv.source = 'opf' AND mv.status::text = 'pending'",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            draft_count >= 5,
            "expected at least 5 draft rows, got {draft_count}"
        );

        // Verify series was created
        let series_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM series_works sw \
             JOIN manifestations m ON m.work_id = sw.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(series_count, 1, "expected series link");
    }

    /// An EPUB with no declared subtitle refine or `numberOfPages` meta must
    /// leave all four fields (both canonical columns and both pointers) NULL
    /// — no colon-split heuristics, no defaulting to zero.
    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_without_subtitle_or_pages_leaves_all_four_null(
        pool: PgPool,
    ) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let source = ingestion.path().join("Tolkien - The Hobbit.epub");
        std::fs::write(&source, make_minimal_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");

        let dest = library.path().join("Tolkien/The Hobbit.epub");
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();

        let (subtitle, subtitle_version_id) = sqlx::query!(
            "SELECT w.subtitle, w.subtitle_version_id FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .map(|r| (r.subtitle, r.subtitle_version_id))
        .unwrap();
        assert!(subtitle.is_none());
        assert!(subtitle_version_id.is_none());

        let (pages, pages_version_id) = sqlx::query!(
            "SELECT pages, pages_version_id FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .map(|r| (r.pages, r.pages_version_id))
        .unwrap();
        assert!(pages.is_none());
        assert!(pages_version_id.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_processes_epub_end_to_end(pool: PgPool) {
        // P1: exercise the EPUB validation path end-to-end, verifying that a clean
        // EPUB gets validation_status='clean' in the manifestation row.
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let source = ingestion.path().join("Tolkien - The Hobbit.epub");
        std::fs::write(&source, make_minimal_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);

        let dest = library.path().join("Tolkien/The Hobbit.epub");
        assert!(dest.exists(), "expected file at {}", dest.display());

        // validation_status must be Clean for a clean EPUB. Decode via the
        // typed enum (not ::text) so the assertion exercises the same
        // ValidationStatus sqlx decode the read paths rely on.
        use crate::models::validation_status::ValidationStatus;
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let status = sqlx::query_scalar!(
            "SELECT validation_status AS \"validation_status!: ValidationStatus\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            status,
            ValidationStatus::Clean,
            "expected validation_status=clean"
        );

        // No cover declared in the OPF manifest — has_embedded_cover must be
        // Some(false), not NULL: the validator ran and found nothing, which is
        // distinct from "never checked".
        let has_cover: Option<bool> = sqlx::query_scalar!(
            "SELECT has_embedded_cover FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(has_cover, Some(false), "expected has_embedded_cover=false");

        let library_id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let hash = copier::hash_file(&source).unwrap();
        let manifestation = sqlx::query!(
            "SELECT id, work_id FROM manifestations WHERE library_id = $1 AND file_path = $2 AND ingestion_file_hash = $3",
            library_id.as_uuid(), dest_str, &hash,
        ).fetch_one(&pool).await.unwrap();
        let app_pool = crate::test_support::db::app_pool_for(&pool).await;
        let (_id, auth) = crate::test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server =
            crate::test_support::db::server_with_opds_enabled(&app_pool, &pool, library.path())
                .await;
        let response = server
            .get(&format!("/opds/books/{}/file", manifestation.id))
            .add_header(axum::http::header::AUTHORIZATION, auth)
            .await;
        let expected = std::fs::read(&dest).unwrap();
        assert_eq!(response.status_code(), axum::http::StatusCode::OK);
        assert_eq!(
            response.header(axum::http::header::CONTENT_LENGTH),
            expected.len().to_string()
        );
        assert_eq!(response.as_bytes().as_ref(), expected);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_epub_with_cover_sets_has_embedded_cover_true(pool: PgPool) {
        // Companion to the dashboard-level coverage test: proves the
        // ingestion pipeline itself sets has_embedded_cover from the
        // validator's Layer 5 cover check, not just that the dashboard query
        // reads the column correctly.
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let source = ingestion.path().join("Cover - Present.epub");
        std::fs::write(&source, make_epub_with_cover()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0);

        let dest = library.path().join("Cover/Present.epub");
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let has_cover: Option<bool> = sqlx::query_scalar!(
            "SELECT has_embedded_cover FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(has_cover, Some(true), "expected has_embedded_cover=true");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_repaired_epub_stores_repaired_status(pool: PgPool) {
        // Cover the ValidationOutcome::Repaired => ValidationStatus::Repaired
        // orchestrator arm end-to-end: an EPUB missing container.xml is repaired
        // in place and the manifestation row must record `repaired`.
        use crate::models::validation_status::ValidationStatus;
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let source = ingestion.path().join("Mended - Patchwork Quilt.epub");
        std::fs::write(&source, make_repaired_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0);

        let dest = library.path().join("Mended/Patchwork Quilt.epub");
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let status = sqlx::query_scalar!(
            "SELECT validation_status AS \"validation_status!: ValidationStatus\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            status,
            ValidationStatus::Repaired,
            "expected validation_status=repaired"
        );
    }

    /// Build an EPUB the validator rates `Repaired` via the `mimetype` OCF
    /// rules: `mimetype` is second in the archive and Deflate-compressed, so
    /// repack rewrites the file and its bytes differ from the source.
    fn make_epub_with_mimetype_second_and_deflated() -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::{ExtendedFileOptions, FileOptions};

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let default: FileOptions<ExtendedFileOptions> = FileOptions::default();

        w.start_file("META-INF/container.xml", default.clone())
            .unwrap();
        w.write_all(CONTAINER_XML).unwrap();

        let mimetype_opts: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("mimetype", mimetype_opts).unwrap();
        w.write_all(b"application/epub+zip").unwrap();

        w.start_file("OEBPS/content.opf", default).unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest/>
  <spine/>
</package>"#,
        )
        .unwrap();

        w.finish().unwrap().into_inner()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_repaired_epub_refreshes_current_hash_and_size(
        pool: PgPool,
    ) {
        // A repaired EPUB's `current_file_hash`/`file_size_bytes` must describe
        // the post-repack library bytes, not the pre-repack copy: `repack`
        // rewrites the file in place, so the copy-time hash and size are stale.
        // `ingestion_file_hash` (the dedup key) must still be the source hash.
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let source_bytes = make_epub_with_mimetype_second_and_deflated();
        let source = ingestion.path().join("Fixed - Broken Mimetype.epub");
        std::fs::write(&source, &source_bytes).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0);

        let dest = library.path().join("Fixed/Broken Mimetype.epub");
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();

        use crate::models::validation_status::ValidationStatus;
        let status = sqlx::query_scalar!(
            "SELECT validation_status AS \"validation_status!: ValidationStatus\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            status,
            ValidationStatus::Repaired,
            "expected validation_status=repaired"
        );

        let (ingestion_hash, current_hash, file_size_bytes) = sqlx::query!(
            "SELECT ingestion_file_hash, current_file_hash, file_size_bytes \
             FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .map(|r| {
            (
                r.ingestion_file_hash,
                r.current_file_hash,
                r.file_size_bytes,
            )
        })
        .unwrap();

        let expected_ingestion_hash = copier::hash_file(&source).unwrap();
        assert_eq!(
            ingestion_hash, expected_ingestion_hash,
            "ingestion_file_hash must equal the source bytes' SHA-256"
        );

        let library_bytes = std::fs::read(&dest).unwrap();
        let expected_current_hash = copier::hash_file(&dest).unwrap();
        assert_eq!(
            current_hash, expected_current_hash,
            "current_file_hash must equal the post-repair library file's SHA-256"
        );

        assert_ne!(
            ingestion_hash, current_hash,
            "repack must change the bytes, so the two hashes must differ"
        );

        assert_eq!(
            file_size_bytes,
            i64::try_from(library_bytes.len()).unwrap(),
            "file_size_bytes must equal the library file's length"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_degraded_epub_stores_degraded_status(pool: PgPool) {
        // Cover the ValidationOutcome::Degraded => ValidationStatus::Degraded
        // orchestrator arm end-to-end: an EPUB declaring a cover whose file is
        // absent is degraded (not repaired) and still ingested.
        use crate::models::validation_status::ValidationStatus;
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let source = ingestion.path().join("Faded - Wilted Garden.epub");
        std::fs::write(&source, make_degraded_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0);

        let dest = library.path().join("Faded/Wilted Garden.epub");
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let status = sqlx::query_scalar!(
            "SELECT validation_status AS \"validation_status!: ValidationStatus\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            status,
            ValidationStatus::Degraded,
            "expected validation_status=degraded"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_validator_error_stores_failed_status(pool: PgPool) {
        // Cover the Ok(Err(_)) validator-crash arm end-to-end via the
        // run_validator fault-injection seam (filename marker): the file is
        // still ingested, but the row must record `failed` — not borrow
        // `degraded`, which means "validator ran, found tolerable issues"
        //.
        use crate::models::validation_status::ValidationStatus;
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let source = ingestion.path().join("Probe - force-validator-error.epub");
        std::fs::write(&source, make_minimal_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0, "validator error must not fail ingestion");

        let dest = library.path().join("Probe/force-validator-error.epub");
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let status = sqlx::query_scalar!(
            "SELECT validation_status AS \"validation_status!: ValidationStatus\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            status,
            ValidationStatus::Failed,
            "expected validation_status=failed for validator crash"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_retains_corrupt_epub_and_reason(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();
        let source = ingestion.path().join("Bad - Corrupt Book.epub");
        std::fs::write(&source, b"this is not a zip file").unwrap();
        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!((result.failed, result.processed), (1, 0));
        let input = crate::models::ingestion_input::current_page(&pool, None)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            input.status,
            crate::models::ingestion_input::InputStatus::Rejected
        );
        assert!(input.reason.as_deref().unwrap().contains("EPUB rejected"));
        assert_eq!(std::fs::read(&source).unwrap(), b"this is not a zip file");
        assert_eq!(std::fs::read_dir(library.path()).unwrap().count(), 0);
        let dest = library.path().join("Bad/Corrupt Book.epub");
        assert!(!dest.exists());
        let relative = "Bad/Corrupt Book.epub";
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM manifestations WHERE file_path = $1",
            relative,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_owner_skips_duplicate_on_second_run(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, _library, config) = scan_env();

        // Unique content to avoid collisions with other test data
        let unique_content = make_minimal_epub();
        let source = ingestion.path().join("Author - Book.epub");
        std::fs::write(&source, &unique_content).unwrap();

        // First scan: should process the file
        let r1 = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(r1.processed, 1, "first scan: expected processed=1");
        assert_eq!(r1.failed, 0);

        // The imported input remains suppressed while its fingerprint is unchanged.
        let r2 = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(r2.skipped, 0, "unchanged imported input is suppressed");
        assert_eq!(r2.processed, 0);
    }

    async fn mixed_cleanup_outcomes(pool: PgPool, imported: bool, duplicates: bool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, mut config) = scan_env();
        let original = ingestion.path().join("Author - Original.epub");
        let duplicate_bytes = make_minimal_epub();
        std::fs::write(&original, &duplicate_bytes).unwrap();
        assert_eq!(drive_owner(&config, &pool).await.unwrap().processed, 1);
        std::fs::remove_file(original).unwrap();
        let completed = ingestion.path().join("Author - Completed.epub");
        let duplicate = ingestion.path().join("Author - Duplicate.epub");
        let rejected = ingestion.path().join("Author - Rejected.epub");
        std::fs::write(&completed, make_metadata_epub()).unwrap();
        std::fs::write(&duplicate, &duplicate_bytes).unwrap();
        std::fs::write(&rejected, b"corrupt archive").unwrap();
        let siblings = [
            "AUTHOR - COMPLETED.txt",
            "Author - Duplicate.txt",
            "Author - Rejected.pdf",
            "unrelated.txt",
        ];
        for name in siblings {
            std::fs::write(ingestion.path().join(name), b"retain").unwrap();
        }
        std::fs::create_dir(ingestion.path().join("unrelated-empty")).unwrap();
        config.cleanup_imported = imported;
        config.cleanup_duplicates = duplicates;
        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!((result.processed, result.skipped, result.failed), (1, 1, 1));
        assert_eq!(completed.exists(), !imported);
        assert_eq!(duplicate.exists(), !duplicates);
        assert_eq!(std::fs::read(rejected).unwrap(), b"corrupt archive");
        for name in siblings {
            assert_eq!(
                std::fs::read(ingestion.path().join(name)).unwrap(),
                b"retain"
            );
        }
        assert!(ingestion.path().join("unrelated-empty").exists());
        assert!(
            library
                .path()
                .join("McAuthor, Test/The Integration Test.epub")
                .exists()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_cleanup_all_outcomes_retained_when_disabled(pool: PgPool) {
        mixed_cleanup_outcomes(pool, false, false).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_cleanup_imported_only_retains_duplicate_rejected_and_siblings(
        pool: PgPool,
    ) {
        mixed_cleanup_outcomes(pool, true, false).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_cleanup_duplicate_opt_in_keeps_rejected_and_siblings(
        pool: PgPool,
    ) {
        mixed_cleanup_outcomes(pool, true, true).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_cleanup_duplicate_only_keeps_imported_original(pool: PgPool) {
        mixed_cleanup_outcomes(pool, false, true).await;
    }

    // ── Task 30: ingest-invariant DB tests ────────────────────────────────

    /// Every non-NULL canonical field set by ingestion must have a matching
    /// `*_version_id` pointer referencing a real `metadata_versions` row with
    /// `source='opf'`.  Without this invariant, `metadata_versions` is optional
    /// instead of authoritative.
    #[sqlx::test(migrations = "./migrations")]
    async fn ingest_sets_version_pointers_for_all_canonical_fields(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let marker = uuid::Uuid::new_v4().simple().to_string();
        let source = ingestion.path().join(format!("invariant-{marker}.epub"));
        std::fs::write(&source, make_metadata_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");

        let dest = library
            .path()
            .join("McAuthor, Test/The Integration Test.epub");
        assert!(dest.exists(), "expected file at {}", dest.display());

        // Pull every canonical field + its pointer in one query.
        // `w.title` is NOT NULL in the schema; force it to nullable via
        // `AS "title?"` so the field type stays `Option<String>` matching
        // the truly-nullable peers. The uniform `if x.is_some()` asserts
        // below then handle every canonical/pointer pair the same way.
        struct Invariant {
            title: Option<String>,
            title_version_id: Option<uuid::Uuid>,
            subtitle: Option<String>,
            subtitle_version_id: Option<uuid::Uuid>,
            language: Option<String>,
            language_version_id: Option<uuid::Uuid>,
            publisher: Option<String>,
            publisher_version_id: Option<uuid::Uuid>,
            pub_date_version_id: Option<uuid::Uuid>,
            isbn_13: Option<String>,
            isbn_13_version_id: Option<uuid::Uuid>,
            pages: Option<i32>,
            pages_version_id: Option<uuid::Uuid>,
        }
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let inv = sqlx::query_as!(
            Invariant,
            "SELECT w.title AS \"title?\", w.title_version_id, \
                    w.subtitle, w.subtitle_version_id, \
                    w.language, w.language_version_id, \
                    m.publisher, m.publisher_version_id, \
                    m.pub_date_version_id, \
                    m.isbn_13, m.isbn_13_version_id, \
                    m.pages, m.pages_version_id \
             FROM manifestations m \
             JOIN works w ON w.id = m.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let Invariant {
            title,
            title_version_id: title_ptr,
            subtitle,
            subtitle_version_id: subtitle_ptr,
            language,
            language_version_id: language_ptr,
            publisher,
            publisher_version_id: publisher_ptr,
            pub_date_version_id: pub_date_ptr,
            isbn_13,
            isbn_13_version_id: isbn_13_ptr,
            pages,
            pages_version_id: pages_ptr,
        } = inv;

        // Invariant: non-NULL canonical value ⇒ non-NULL pointer.
        if title.is_some() {
            assert!(title_ptr.is_some(), "title set but title_version_id NULL");
        }
        if subtitle.is_some() {
            assert!(
                subtitle_ptr.is_some(),
                "subtitle set but subtitle_version_id NULL"
            );
        }
        if language.is_some() {
            assert!(
                language_ptr.is_some(),
                "language set but language_version_id NULL"
            );
        }
        if publisher.is_some() {
            assert!(
                publisher_ptr.is_some(),
                "publisher set but publisher_version_id NULL"
            );
        }
        if isbn_13.is_some() {
            assert!(
                isbn_13_ptr.is_some(),
                "isbn_13 set but isbn_13_version_id NULL"
            );
        }
        if pages.is_some() {
            assert!(pages_ptr.is_some(), "pages set but pages_version_id NULL");
        }

        // Every non-NULL pointer must reference a real source='opf' row.
        for pointer in [
            title_ptr,
            subtitle_ptr,
            language_ptr,
            publisher_ptr,
            pub_date_ptr,
            isbn_13_ptr,
            pages_ptr,
        ]
        .into_iter()
        .flatten()
        {
            let source_for_ptr = sqlx::query_scalar!(
                "SELECT source FROM metadata_versions WHERE id = $1",
                pointer,
            )
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| {
                panic!("pointer {pointer} did not resolve to a metadata_versions row: {e}")
            });
            assert_eq!(
                source_for_ptr, "opf",
                "pointer {pointer} resolved to source '{source_for_ptr}', expected 'opf'"
            );
        }
    }

    /// When ingestion cannot extract OPF (e.g. for a non-EPUB file), a
    /// heuristic-fallback row is written to `metadata_versions` with
    /// `source='opf'`, `field_name='title'`, `confidence_score=0.2` and the
    /// work's `title_version_id` pointer references it.
    #[sqlx::test(migrations = "./migrations")]
    async fn ingest_without_opf_writes_heuristic_title_journal(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let marker = uuid::Uuid::new_v4().simple().to_string();
        let source = ingestion.path().join(format!(
            "Heuristic Author - Heuristic Title force-validator-error {marker}.epub"
        ));
        std::fs::write(&source, make_minimal_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");

        let dest = library.path().join(format!(
            "Heuristic Author/Heuristic Title force-validator-error {marker}.epub"
        ));
        assert!(dest.exists(), "expected file at {}", dest.display());

        // The work should have its title_version_id pointing at the heuristic
        // row, which must have source='opf', field_name='title', confidence=0.2.
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let row = sqlx::query!(
            "SELECT w.title_version_id, w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            row.title.contains("Heuristic Title"),
            "title should include heuristic value, got '{}'",
            row.title,
        );
        let ptr = row
            .title_version_id
            .expect("title_version_id must be wired for heuristic row");

        let row = sqlx::query!(
            "SELECT source, field_name, confidence_score \
             FROM metadata_versions WHERE id = $1",
            ptr,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.source, "opf");
        assert_eq!(row.field_name, "title");
        let score = row.confidence_score;
        assert!(
            (score - 0.2).abs() < 1e-4,
            "heuristic confidence should be ~0.2, got {score}"
        );
    }

    /// `work_authors.source_version_id` must be wired to the per-role
    /// `contributors.author` journal row so authors on the work trace back
    /// to their draft.
    #[sqlx::test(migrations = "./migrations")]
    async fn ingest_sets_work_authors_source_version_id(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (ingestion, library, config) = scan_env();

        let marker = uuid::Uuid::new_v4().simple().to_string();
        let source = ingestion.path().join(format!("authors-{marker}.epub"));
        std::fs::write(&source, make_metadata_epub()).unwrap();

        let result = drive_owner(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");

        let dest = library
            .path()
            .join("McAuthor, Test/The Integration Test.epub");

        // Every work_author row for this work must carry a source_version_id
        // pointing at a metadata_versions row with
        // field_name='contributors.author'.
        let dest_str = dest.strip_prefix(library.path()).unwrap().to_str().unwrap();
        let rows = sqlx::query!(
            "SELECT wa.author_id, wa.source_version_id \
             FROM work_authors wa \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!rows.is_empty(), "expected at least one work_author row");

        for row in rows {
            let ptr = row.source_version_id.unwrap_or_else(|| {
                panic!(
                    "work_authors.source_version_id NULL for author {}",
                    row.author_id
                )
            });
            let field_name = sqlx::query_scalar!(
                "SELECT field_name FROM metadata_versions WHERE id = $1",
                ptr,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                field_name, "contributors.author",
                "source_version_id should reference a 'contributors.author' journal row"
            );
        }
    }
}

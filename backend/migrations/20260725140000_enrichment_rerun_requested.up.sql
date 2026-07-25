-- Decouple "re-run enrichment after this edit" from the queue's claim state.
--
-- An identifier edit re-queues enrichment, but flipping an in_progress row
-- straight to pending has two failure modes: another worker can claim the row
-- while the original run is still active (two concurrent runs mutating the
-- same manifestation), and if the original run finishes first its completion
-- bookkeeping overwrites the pending status, silently dropping a re-run that
-- is still needed because the active run snapshotted its lookup keys before
-- the edit. An edit that lands while a run is active now sets this flag
-- instead of touching the status; the worker's completion path converts the
-- flag into a fresh, immediately eligible pending row.
ALTER TABLE public.manifestations
    ADD COLUMN enrichment_rerun_requested boolean DEFAULT false NOT NULL;

-- Restore the single-column index dropped in the up-migration (exact original
-- definition from initial_schema.up.sql).
CREATE INDEX idx_device_tokens_user_id ON public.device_tokens USING btree (user_id);

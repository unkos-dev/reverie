-- Add 'failed' to validation_status: the validator itself could not run
-- (internal error / crash), as opposed to 'degraded' (validator ran, found
-- tolerable issues) or 'pending' (validation not attempted). Previously the
-- validator-crash path borrowed 'degraded', so an operator monitoring
-- degraded files could not distinguish "questionable file" from "validator
-- never ran" (UNK-312). Appended after 'degraded'; enum order is not load-
-- bearing (no ORDER BY on this column).
ALTER TYPE public.validation_status ADD VALUE 'failed';

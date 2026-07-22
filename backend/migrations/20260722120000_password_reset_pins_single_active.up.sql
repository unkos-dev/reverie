-- Enforce the single-active-recovery-PIN invariant at the schema level.
--
-- password_reset_pin::rotate supersedes a user's prior unconsumed PINs (a
-- DELETE WHERE consumed_at IS NULL) and inserts a fresh one inside one
-- transaction. The intended invariant is "at most one active PIN per user,"
-- but the plain idx_password_reset_pins_user_id index does not enforce it:
-- under READ COMMITTED two concurrent forgot-password requests for the same
-- account can each run the supersede DELETE without seeing the other's
-- uncommitted INSERT, then both commit, leaving two live PINs for one user.
--
-- This partial unique index makes that state unrepresentable. Only unconsumed
-- rows participate (WHERE consumed_at IS NULL), so a user may accumulate any
-- number of consumed (spent) rows while at most one stays active. The losing
-- concurrent issuer's INSERT blocks on the winner's index tuple and then fails
-- with SQLSTATE 23505; rotate handles that as a benign race. The plain
-- user_id index is retained: it still backs the FK-cascade enumeration and the
-- active-PIN lookup's inequality predicates that this partial index does not
-- fully cover.
CREATE UNIQUE INDEX idx_password_reset_pins_active_unique
    ON public.password_reset_pins USING btree (user_id)
    WHERE (consumed_at IS NULL);

COMMENT ON INDEX public.idx_password_reset_pins_active_unique IS 'Enforces at most one active (unconsumed) recovery PIN per user. Load-bearing for the single-active invariant under concurrency; see models::password_reset_pin::rotate for the retry path on SQLSTATE 23505.';

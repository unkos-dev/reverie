-- Reverse of the local-auth recovery/throttle/bootstrap migration, in inverse
-- order. Local-dev reversibility only, not a production rollback; these
-- migrations roll up into the base schema before the first release.

DROP TRIGGER IF EXISTS trg_local_login_throttle_updated_at ON public.local_login_throttle;

DROP TABLE public.instance_bootstrap;
DROP TABLE public.local_login_throttle;
DROP TABLE public.password_reset_pins;

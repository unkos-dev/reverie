UPDATE public.settings
SET accepted_formats = '{epub}',
    revision = revision + 1,
    updated_at = CURRENT_TIMESTAMP
WHERE accepted_formats = '{}';

ALTER TABLE public.settings
ADD CONSTRAINT settings_accepted_formats_nonempty CHECK (accepted_formats <> '{}');

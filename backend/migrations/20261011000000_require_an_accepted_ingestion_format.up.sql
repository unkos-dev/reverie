UPDATE public.settings
SET accepted_formats = '{epub}'
WHERE accepted_formats = '{}';

ALTER TABLE public.settings
ADD CONSTRAINT settings_accepted_formats_nonempty CHECK (accepted_formats <> '{}');

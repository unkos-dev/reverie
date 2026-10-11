UPDATE public.settings
SET accepted_formats = '{epub}'
WHERE cardinality(accepted_formats) = 0;

ALTER TABLE public.settings
ADD CONSTRAINT settings_accepted_formats_nonempty CHECK (cardinality(accepted_formats) >= 1);

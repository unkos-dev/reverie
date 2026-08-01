-- series_works.position was the schema's only NUMERIC column while every
-- writer already binds ::float8 and every reader decodes f64, so the type
-- bought no precision and cost a per-query cast discipline enforced
-- nowhere: a missed cast decoded raw NUMERIC bytes as garbage doubles in
-- macro queries and silently nulled the field in runtime-checked ones.
-- double precision removes the cast requirement and the out-of-float8-range
-- evaluation hazard together. The conversion is lossless: every stored
-- value arrived through a float8 bind.
ALTER TABLE public.series_works
ALTER COLUMN position TYPE double precision USING position::float8;

-- Reverse the validation_status 'valid' -> 'clean' rename (UNK-276).
ALTER TYPE public.validation_status RENAME VALUE 'clean' TO 'valid';

ALTER TABLE public.manifestations
    DROP CONSTRAINT manifestations_library_file_path_key,
    DROP CONSTRAINT manifestations_relative_file_path_check,
    DROP COLUMN library_id,
    ADD CONSTRAINT manifestations_file_path_key UNIQUE (file_path);

DROP TABLE public.libraries;

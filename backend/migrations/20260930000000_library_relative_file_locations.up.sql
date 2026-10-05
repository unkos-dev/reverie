CREATE TABLE public.libraries (
    id uuid PRIMARY KEY DEFAULT uuidv7(), -- noqa: CV11
    configuration_key text UNIQUE NOT NULL CHECK (configuration_key <> '')
);

INSERT INTO public.libraries (configuration_key) VALUES ('default');

ALTER TABLE public.manifestations
    DROP CONSTRAINT manifestations_file_path_key,
    ADD COLUMN library_id uuid NOT NULL REFERENCES public.libraries(id) ON DELETE RESTRICT,
    ADD CONSTRAINT manifestations_library_file_path_key UNIQUE (library_id, file_path),
    ADD CONSTRAINT manifestations_relative_file_path_check CHECK (
        file_path <> ''
        AND left(file_path, 1) <> '/' -- noqa: CV11
        AND right(file_path, 1) <> '/' -- noqa: CV11
        AND file_path NOT LIKE '%//%'
        AND strpos(file_path, chr(92)) = 0 -- noqa: CV11
        AND file_path !~ '^[A-Za-z]:'
        AND file_path !~ '(^|/)[.]{1,2}(/|$)'
    );

GRANT SELECT ON TABLE public.libraries TO reverie_app, reverie_ingestion, reverie_readonly;

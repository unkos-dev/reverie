ALTER TABLE public.settings
    ADD COLUMN openlibrary_base_url text DEFAULT 'https://openlibrary.org'::text NOT NULL,
    ADD COLUMN googlebooks_base_url text DEFAULT 'https://www.googleapis.com/books/v1'::text NOT NULL,
    ADD COLUMN hardcover_base_url text DEFAULT 'https://api.hardcover.app/v1/graphql'::text NOT NULL;

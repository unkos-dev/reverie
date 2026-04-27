CREATE TYPE theme_preference AS ENUM ('system', 'light', 'dark');

ALTER TABLE users
    ADD COLUMN theme_preference theme_preference NOT NULL DEFAULT 'system';

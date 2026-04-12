REVOKE ALL ON shelf_items FROM tome_readonly;
REVOKE ALL ON shelves FROM tome_readonly;

REVOKE ALL ON device_tokens FROM tome_app;
REVOKE ALL ON shelf_items FROM tome_app;
REVOKE ALL ON shelves FROM tome_app;

DROP TABLE IF EXISTS device_tokens;
DROP TABLE IF EXISTS shelf_items;
DROP TABLE IF EXISTS shelves;

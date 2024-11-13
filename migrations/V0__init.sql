CREATE TABLE configuration (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT
);

CREATE TABLE import_log (
    file_name TEXT,
    transaction_ids TEXT,
    insert_datetime DATETIME DEFAULT CURRENT_TIMESTAMP
);
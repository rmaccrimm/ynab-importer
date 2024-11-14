CREATE TABLE budget (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

CREATE TABLE account (
    budget_id INTEGER NOT NULL REFERENCES budget(id),
    uuid TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

CREATE TABLE configuration (
    budget_id INTEGER NOT NULL REFERENCES budget(id),
    key NOT NULL,
    value TEXT,
    PRIMARY KEY(budget_id, key)
);

CREATE TABLE import_log (
    budget_id INTEGER NOT NULL REFERENCES budget(id),
    file_name TEXT,
    transaction_ids TEXT,
    insert_datetime DATETIME DEFAULT CURRENT_TIMESTAMP
);
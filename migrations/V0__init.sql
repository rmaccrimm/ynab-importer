CREATE TABLE budget (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE account (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    budget_id INTEGER NOT NULL REFERENCES budget(id),
    uuid TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

CREATE TABLE transaction_import (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    amount INTEGER NOT NULL,
    date_posted TEXT NOT NULL,
    account_id INTEGER NOT NULL REFERENCES account(id),
    UNIQUE(amount, date_posted, account_id)
);

CREATE TABLE configuration (
    key TEXT PRIMARY KEY,
    value TEXT
);
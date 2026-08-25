CREATE TABLE budgets (
    name TEXT PRIMARY KEY NOT NULL,
    amount TEXT NOT NULL,
    currency TEXT NOT NULL,
    period TEXT NOT NULL CHECK (period IN ('daily', 'weekly', 'monthly')),
    provider TEXT,
    model TEXT,
    project TEXT,
    source TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_budgets_period ON budgets(period, name);

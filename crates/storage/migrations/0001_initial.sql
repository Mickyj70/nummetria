CREATE TABLE usage_records (
    id TEXT PRIMARY KEY NOT NULL,
    schema_version INTEGER NOT NULL,
    provider TEXT NOT NULL,
    model TEXT,
    project TEXT,
    period_start TEXT NOT NULL,
    period_end TEXT NOT NULL,
    collected_at TEXT NOT NULL,
    cost_evidence TEXT NOT NULL CHECK (
        cost_evidence IN ('reported', 'calculated', 'estimated', 'unknown')
    ),
    cost_amount TEXT,
    cost_currency TEXT,
    pricing_reference TEXT,
    payload TEXT NOT NULL,
    CHECK (period_end > period_start),
    CHECK (
        (cost_evidence = 'unknown'
            AND cost_amount IS NULL
            AND cost_currency IS NULL
            AND pricing_reference IS NULL)
        OR
        (cost_evidence = 'reported'
            AND cost_amount IS NOT NULL
            AND cost_currency IS NOT NULL
            AND pricing_reference IS NULL)
        OR
        (cost_evidence IN ('calculated', 'estimated')
            AND cost_amount IS NOT NULL
            AND cost_currency IS NOT NULL
            AND pricing_reference IS NOT NULL)
    )
);

CREATE TABLE usage_quantities (
    record_id TEXT NOT NULL REFERENCES usage_records(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL,
    amount TEXT NOT NULL,
    PRIMARY KEY (record_id, position)
);

CREATE TABLE collection_checkpoints (
    provider TEXT NOT NULL,
    stream TEXT NOT NULL,
    cursor TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (provider, stream)
);

CREATE INDEX idx_usage_records_period
    ON usage_records(period_start, period_end);
CREATE INDEX idx_usage_records_provider_period
    ON usage_records(provider, period_start);
CREATE INDEX idx_usage_records_model_period
    ON usage_records(model, period_start)
    WHERE model IS NOT NULL;
CREATE INDEX idx_usage_records_project_period
    ON usage_records(project, period_start)
    WHERE project IS NOT NULL;

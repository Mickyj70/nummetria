//! Local SQLite persistence for Nummetria.
//!
//! The storage layer accepts only validated core-domain records. Writes are
//! transactional, repeated identical records are idempotent, and a reused
//! record ID with different contents is reported as a conflict.

use std::{collections::HashMap, path::Path};

use chrono::{DateTime, SecondsFormat, Utc};
use nummetria_core::{
    Cost, CostEvidence, CurrencyCode, ModelId, ProjectId, ProviderId, RecordId, UsageKind,
    UsageRecord,
};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params, params_from_iter, types::Value};
use rust_decimal::Decimal;
use thiserror::Error;

const LATEST_SCHEMA_VERSION: u32 = 1;
const MIGRATIONS: &[(u32, &str)] = &[(1, include_str!("../migrations/0001_initial.sql"))];

/// Failures from opening, migrating, reading, or writing Nummetria's database.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("could not serialize or deserialize a usage record: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    NewerSchema { found: u32, supported: u32 },
    #[error("record ID {id} already exists with different contents")]
    RecordConflict { id: String },
    #[error("query end must be later than query start")]
    InvalidQueryRange,
    #[error("checkpoint stream cannot be empty")]
    EmptyCheckpointStream,
    #[error("backup destination already exists: {0}")]
    BackupDestinationExists(String),
}

/// Whether an insert created a row or recognized an identical existing row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    AlreadyPresent,
}

/// Summary of a transactional batch insertion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InsertSummary {
    pub inserted: usize,
    pub already_present: usize,
}

/// Optional filters for usage records. Time bounds select overlapping records.
#[derive(Debug, Clone, Default)]
pub struct UsageQuery<'a> {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub provider: Option<&'a ProviderId>,
    pub model: Option<&'a ModelId>,
    pub project: Option<&'a ProjectId>,
}

impl UsageQuery<'_> {
    fn validate(&self) -> Result<(), StorageError> {
        if let (Some(start), Some(end)) = (self.start, self.end) {
            if end <= start {
                return Err(StorageError::InvalidQueryRange);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantityTotal {
    pub kind: UsageKind,
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostTotal {
    pub evidence: CostEvidence,
    pub currency: CurrencyCode,
    pub amount: Decimal,
}

/// Provider-neutral totals calculated without converting decimals to floats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageAggregate {
    pub record_count: usize,
    pub quantities: Vec<QuantityTotal>,
    pub costs: Vec<CostTotal>,
    pub unknown_cost_record_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionCheckpoint {
    pub provider: ProviderId,
    pub stream: String,
    pub cursor: String,
    pub updated_at: DateTime<Utc>,
}

/// A migrated connection to Nummetria's local SQLite database.
pub struct SqliteStorage {
    connection: Connection,
}

impl SqliteStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, StorageError> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\nPRAGMA busy_timeout = 5000;\nPRAGMA journal_mode = WAL;",
        )?;
        migrate(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn insert_usage_record(
        &mut self,
        record: &UsageRecord,
    ) -> Result<InsertOutcome, StorageError> {
        let transaction = self.connection.transaction()?;
        let outcome = insert_record(&transaction, record)?;
        transaction.commit()?;
        Ok(outcome)
    }

    pub fn insert_usage_records(
        &mut self,
        records: &[UsageRecord],
    ) -> Result<InsertSummary, StorageError> {
        let transaction = self.connection.transaction()?;
        let mut summary = InsertSummary::default();

        for record in records {
            match insert_record(&transaction, record)? {
                InsertOutcome::Inserted => summary.inserted += 1,
                InsertOutcome::AlreadyPresent => summary.already_present += 1,
            }
        }

        transaction.commit()?;
        Ok(summary)
    }

    /// Atomically inserts provider observations and advances collection checkpoints.
    pub fn insert_usage_records_with_checkpoints(
        &mut self,
        records: &[UsageRecord],
        checkpoints: &[CollectionCheckpoint],
    ) -> Result<InsertSummary, StorageError> {
        if checkpoints
            .iter()
            .any(|checkpoint| checkpoint.stream.trim().is_empty())
        {
            return Err(StorageError::EmptyCheckpointStream);
        }

        let transaction = self.connection.transaction()?;
        let mut summary = InsertSummary::default();
        for record in records {
            match insert_record(&transaction, record)? {
                InsertOutcome::Inserted => summary.inserted += 1,
                InsertOutcome::AlreadyPresent => summary.already_present += 1,
            }
        }
        for checkpoint in checkpoints {
            transaction.execute(
                "INSERT INTO collection_checkpoints (provider, stream, cursor, updated_at)\n\
                 VALUES (?1, ?2, ?3, ?4)\n\
                 ON CONFLICT(provider, stream) DO UPDATE SET\n\
                     cursor = excluded.cursor, updated_at = excluded.updated_at",
                params![
                    checkpoint.provider.as_str(),
                    checkpoint.stream,
                    checkpoint.cursor,
                    timestamp(checkpoint.updated_at),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(summary)
    }

    pub fn get_usage_record(&self, id: &RecordId) -> Result<Option<UsageRecord>, StorageError> {
        let payload = self
            .connection
            .query_row(
                "SELECT payload FROM usage_records WHERE id = ?1",
                [id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        payload
            .map(|payload| serde_json::from_str(&payload).map_err(StorageError::from))
            .transpose()
    }

    pub fn query_usage(&self, query: &UsageQuery<'_>) -> Result<Vec<UsageRecord>, StorageError> {
        query.validate()?;
        let mut sql = String::from("SELECT payload FROM usage_records WHERE 1 = 1");
        let mut values = Vec::<Value>::new();

        if let Some(start) = query.start {
            sql.push_str(" AND period_end > ?");
            values.push(Value::Text(timestamp(start)));
        }
        if let Some(end) = query.end {
            sql.push_str(" AND period_start < ?");
            values.push(Value::Text(timestamp(end)));
        }
        if let Some(provider) = query.provider {
            sql.push_str(" AND provider = ?");
            values.push(Value::Text(provider.as_str().to_owned()));
        }
        if let Some(model) = query.model {
            sql.push_str(" AND model = ?");
            values.push(Value::Text(model.as_str().to_owned()));
        }
        if let Some(project) = query.project {
            sql.push_str(" AND project = ?");
            values.push(Value::Text(project.as_str().to_owned()));
        }
        sql.push_str(" ORDER BY period_start, id");

        let mut statement = self.connection.prepare(&sql)?;
        let payloads = statement
            .query_map(params_from_iter(values), |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        payloads
            .into_iter()
            .map(|payload| serde_json::from_str(&payload).map_err(StorageError::from))
            .collect()
    }

    pub fn aggregate_usage(&self, query: &UsageQuery<'_>) -> Result<UsageAggregate, StorageError> {
        let records = self.query_usage(query)?;
        let mut quantity_totals = HashMap::<UsageKind, Decimal>::new();
        let mut cost_totals = HashMap::<(CostEvidence, CurrencyCode), Decimal>::new();
        let mut unknown_cost_record_count = 0;

        for record in &records {
            for quantity in &record.quantities {
                *quantity_totals.entry(quantity.kind).or_default() += quantity.amount;
            }

            match &record.cost {
                Cost::Reported { amount, currency } => {
                    *cost_totals
                        .entry((CostEvidence::Reported, currency.clone()))
                        .or_default() += *amount;
                }
                Cost::Calculated {
                    amount, currency, ..
                } => {
                    *cost_totals
                        .entry((CostEvidence::Calculated, currency.clone()))
                        .or_default() += *amount;
                }
                Cost::Estimated {
                    amount, currency, ..
                } => {
                    *cost_totals
                        .entry((CostEvidence::Estimated, currency.clone()))
                        .or_default() += *amount;
                }
                Cost::Unknown => unknown_cost_record_count += 1,
            }
        }

        let mut quantities = quantity_totals
            .into_iter()
            .map(|(kind, amount)| QuantityTotal { kind, amount })
            .collect::<Vec<_>>();
        quantities.sort_by_key(|total| usage_kind_name(total.kind));

        let mut costs = cost_totals
            .into_iter()
            .map(|((evidence, currency), amount)| CostTotal {
                evidence,
                currency,
                amount,
            })
            .collect::<Vec<_>>();
        costs.sort_by(|left, right| {
            cost_evidence_name(&left.evidence)
                .cmp(cost_evidence_name(&right.evidence))
                .then_with(|| left.currency.as_str().cmp(right.currency.as_str()))
        });

        Ok(UsageAggregate {
            record_count: records.len(),
            quantities,
            costs,
            unknown_cost_record_count,
        })
    }

    pub fn set_checkpoint(
        &mut self,
        checkpoint: &CollectionCheckpoint,
    ) -> Result<(), StorageError> {
        if checkpoint.stream.trim().is_empty() {
            return Err(StorageError::EmptyCheckpointStream);
        }

        self.connection.execute(
            "INSERT INTO collection_checkpoints (provider, stream, cursor, updated_at)\n\
             VALUES (?1, ?2, ?3, ?4)\n\
             ON CONFLICT(provider, stream) DO UPDATE SET\n\
                 cursor = excluded.cursor, updated_at = excluded.updated_at",
            params![
                checkpoint.provider.as_str(),
                checkpoint.stream,
                checkpoint.cursor,
                timestamp(checkpoint.updated_at),
            ],
        )?;
        Ok(())
    }

    pub fn get_checkpoint(
        &self,
        provider: &ProviderId,
        stream: &str,
    ) -> Result<Option<CollectionCheckpoint>, StorageError> {
        let stored = self
            .connection
            .query_row(
                "SELECT cursor, updated_at FROM collection_checkpoints\n\
                 WHERE provider = ?1 AND stream = ?2",
                params![provider.as_str(), stream],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        stored
            .map(|(cursor, updated_at)| {
                Ok(CollectionCheckpoint {
                    provider: provider.clone(),
                    stream: stream.to_owned(),
                    cursor,
                    updated_at: parse_timestamp(&updated_at)?,
                })
            })
            .transpose()
    }

    /// Creates a consistent SQLite backup and refuses to overwrite a file.
    pub fn backup_to(&self, destination: impl AsRef<Path>) -> Result<(), StorageError> {
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(StorageError::BackupDestinationExists(
                destination.display().to_string(),
            ));
        }
        self.connection.backup(MAIN_DB, destination, None)?;
        Ok(())
    }

    /// Deletes records whose interval ends at or before the UTC cutoff.
    pub fn delete_usage_before(&mut self, cutoff: DateTime<Utc>) -> Result<usize, StorageError> {
        Ok(self.connection.execute(
            "DELETE FROM usage_records WHERE period_end <= ?1",
            [timestamp(cutoff)],
        )?)
    }

    /// Deletes all collected usage and checkpoints while retaining the schema.
    pub fn delete_all_data(&mut self) -> Result<(), StorageError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM usage_records", [])?;
        transaction.execute("DELETE FROM collection_checkpoints", [])?;
        transaction.commit()?;
        Ok(())
    }

    #[cfg(test)]
    fn row_count(&self, table: &str) -> usize {
        let count: i64 = self
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        usize::try_from(count).unwrap()
    }
}

fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(StorageError::NewerSchema {
            found: current,
            supported: LATEST_SCHEMA_VERSION,
        });
    }

    for (version, sql) in MIGRATIONS.iter().filter(|(version, _)| *version > current) {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }

    connection.execute_batch("PRAGMA optimize;")?;
    Ok(())
}

fn insert_record(
    transaction: &rusqlite::Transaction<'_>,
    record: &UsageRecord,
) -> Result<InsertOutcome, StorageError> {
    let payload = serde_json::to_string(record)?;
    let (cost_evidence, cost_amount, cost_currency, pricing_reference) = cost_columns(&record.cost);
    let inserted = transaction.execute(
        "INSERT OR IGNORE INTO usage_records (\n\
             id, schema_version, provider, model, project, period_start, period_end, collected_at,\n\
             cost_evidence, cost_amount, cost_currency, pricing_reference, payload\n\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            record.id.as_str(),
            record.schema_version,
            record.provider.as_str(),
            record.model.as_ref().map(ModelId::as_str),
            record.project.as_ref().map(ProjectId::as_str),
            timestamp(record.time_range.start),
            timestamp(record.time_range.end),
            timestamp(record.collected_at),
            cost_evidence,
            cost_amount,
            cost_currency,
            pricing_reference,
            payload,
        ],
    )?;

    if inserted == 0 {
        let existing: String = transaction.query_row(
            "SELECT payload FROM usage_records WHERE id = ?1",
            [record.id.as_str()],
            |row| row.get(0),
        )?;
        return if existing == payload {
            Ok(InsertOutcome::AlreadyPresent)
        } else {
            Err(StorageError::RecordConflict {
                id: record.id.as_str().to_owned(),
            })
        };
    }

    for (position, quantity) in record.quantities.iter().enumerate() {
        transaction.execute(
            "INSERT INTO usage_quantities (record_id, position, kind, amount)\n\
             VALUES (?1, ?2, ?3, ?4)",
            params![
                record.id.as_str(),
                position as i64,
                usage_kind_name(quantity.kind),
                quantity.amount.to_string(),
            ],
        )?;
    }

    Ok(InsertOutcome::Inserted)
}

fn cost_columns(cost: &Cost) -> (&'static str, Option<String>, Option<&str>, Option<&str>) {
    match cost {
        Cost::Reported { amount, currency } => (
            "reported",
            Some(amount.to_string()),
            Some(currency.as_str()),
            None,
        ),
        Cost::Calculated {
            amount,
            currency,
            pricing_reference,
        } => (
            "calculated",
            Some(amount.to_string()),
            Some(currency.as_str()),
            Some(pricing_reference),
        ),
        Cost::Estimated {
            amount,
            currency,
            pricing_reference,
        } => (
            "estimated",
            Some(amount.to_string()),
            Some(currency.as_str()),
            Some(pricing_reference),
        ),
        Cost::Unknown => ("unknown", None, None, None),
    }
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            StorageError::Database(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            ))
        })
}

fn usage_kind_name(kind: UsageKind) -> &'static str {
    match kind {
        UsageKind::InputTokens => "input_tokens",
        UsageKind::OutputTokens => "output_tokens",
        UsageKind::CachedTokens => "cached_tokens",
        UsageKind::CacheWriteTokens => "cache_write_tokens",
        UsageKind::ReasoningTokens => "reasoning_tokens",
        UsageKind::Requests => "requests",
        UsageKind::Images => "images",
        UsageKind::AudioSeconds => "audio_seconds",
        UsageKind::VideoSeconds => "video_seconds",
        UsageKind::ToolCalls => "tool_calls",
        UsageKind::WebSearches => "web_searches",
        UsageKind::ComputeSeconds => "compute_seconds",
    }
}

fn cost_evidence_name(evidence: &CostEvidence) -> &'static str {
    match evidence {
        CostEvidence::Reported => "reported",
        CostEvidence::Calculated => "calculated",
        CostEvidence::Estimated => "estimated",
        CostEvidence::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::{Duration, TimeZone};
    use nummetria_core::{CollectionSource, TimeRange, UsageQuantity};
    use tempfile::tempdir;

    use super::*;

    fn instant(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, hour, 0, 0).unwrap()
    }

    fn record(id: &str, provider: &str, day: u32, cost: Cost) -> UsageRecord {
        UsageRecord::new(
            RecordId::new(id).unwrap(),
            ProviderId::new(provider).unwrap(),
            Some(ModelId::new("model-a").unwrap()),
            Some(ProjectId::new("project-a").unwrap()),
            TimeRange::new(instant(day, 0), instant(day + 1, 0)).unwrap(),
            vec![
                UsageQuantity::new(UsageKind::InputTokens, Decimal::new(1250, 0)).unwrap(),
                UsageQuantity::new(UsageKind::Requests, Decimal::ONE).unwrap(),
            ],
            cost,
            CollectionSource::ProviderApi {
                operation: "usage".into(),
            },
            instant(day + 1, 1),
        )
        .unwrap()
    }

    fn reported(amount: &str) -> Cost {
        Cost::Reported {
            amount: Decimal::from_str(amount).unwrap(),
            currency: CurrencyCode::new("USD").unwrap(),
        }
    }

    #[test]
    fn migrates_new_databases_and_reopens_them() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("usage.db");

        let storage = SqliteStorage::open(&path).unwrap();
        assert_eq!(storage.schema_version().unwrap(), 1);
        drop(storage);

        let reopened = SqliteStorage::open(&path).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), 1);
    }

    #[test]
    fn repeated_identical_inserts_are_idempotent() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let usage = record("same-id", "openai", 17, reported("0.03125"));

        assert_eq!(
            storage.insert_usage_record(&usage).unwrap(),
            InsertOutcome::Inserted
        );
        assert_eq!(
            storage.insert_usage_record(&usage).unwrap(),
            InsertOutcome::AlreadyPresent
        );
        assert_eq!(storage.row_count("usage_records"), 1);
        assert_eq!(storage.row_count("usage_quantities"), 2);
    }

    #[test]
    fn reused_identity_with_different_contents_is_a_conflict() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let first = record("same-id", "openai", 17, reported("0.03"));
        let changed = record("same-id", "openai", 17, reported("0.04"));

        storage.insert_usage_record(&first).unwrap();
        assert!(matches!(
            storage.insert_usage_record(&changed),
            Err(StorageError::RecordConflict { .. })
        ));
        assert_eq!(storage.get_usage_record(&first.id).unwrap(), Some(first));
    }

    #[test]
    fn batch_conflicts_roll_back_the_entire_transaction() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let existing = record("existing", "openai", 17, reported("0.03"));
        storage.insert_usage_record(&existing).unwrap();
        let new = record("new", "anthropic", 18, Cost::Unknown);
        let conflict = record("existing", "openai", 17, reported("0.04"));

        assert!(
            storage
                .insert_usage_records(&[new.clone(), conflict])
                .is_err()
        );
        assert_eq!(storage.get_usage_record(&new.id).unwrap(), None);
    }

    #[test]
    fn queries_overlapping_utc_ranges_and_provider_filters() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let openai = record("openai-17", "openai", 17, reported("0.03125"));
        let anthropic = record("anthropic-18", "anthropic", 18, Cost::Unknown);
        storage
            .insert_usage_records(&[openai.clone(), anthropic])
            .unwrap();
        let provider = ProviderId::new("openai").unwrap();

        let found = storage
            .query_usage(&UsageQuery {
                start: Some(instant(17, 12)),
                end: Some(instant(18, 12)),
                provider: Some(&provider),
                ..UsageQuery::default()
            })
            .unwrap();

        assert_eq!(found, vec![openai]);
    }

    #[test]
    fn aggregates_decimal_usage_and_keeps_cost_evidence_separate() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        storage
            .insert_usage_records(&[
                record("reported", "openai", 17, reported("0.03125")),
                record(
                    "estimated",
                    "openai",
                    18,
                    Cost::Estimated {
                        amount: Decimal::from_str("0.02001").unwrap(),
                        currency: CurrencyCode::new("USD").unwrap(),
                        pricing_reference: "pricing-2026-08".into(),
                    },
                ),
                record("unknown", "anthropic", 19, Cost::Unknown),
            ])
            .unwrap();

        let aggregate = storage.aggregate_usage(&UsageQuery::default()).unwrap();

        assert_eq!(aggregate.record_count, 3);
        assert_eq!(aggregate.unknown_cost_record_count, 1);
        assert!(aggregate.quantities.iter().any(|total| {
            total.kind == UsageKind::InputTokens && total.amount == Decimal::new(3750, 0)
        }));
        assert!(aggregate.costs.iter().any(|total| {
            total.evidence == CostEvidence::Reported
                && total.amount == Decimal::from_str("0.03125").unwrap()
        }));
        assert!(aggregate.costs.iter().any(|total| {
            total.evidence == CostEvidence::Estimated
                && total.amount == Decimal::from_str("0.02001").unwrap()
        }));
    }

    #[test]
    fn checkpoints_update_by_provider_and_stream() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let provider = ProviderId::new("openai").unwrap();
        let mut checkpoint = CollectionCheckpoint {
            provider: provider.clone(),
            stream: "organization-usage".into(),
            cursor: "page-1".into(),
            updated_at: instant(18, 1),
        };
        storage.set_checkpoint(&checkpoint).unwrap();
        checkpoint.cursor = "page-2".into();
        checkpoint.updated_at += Duration::minutes(1);
        storage.set_checkpoint(&checkpoint).unwrap();

        assert_eq!(
            storage
                .get_checkpoint(&provider, "organization-usage")
                .unwrap(),
            Some(checkpoint)
        );
    }

    #[test]
    fn provider_records_and_checkpoints_commit_atomically() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let provider = ProviderId::new("openai").unwrap();
        let checkpoint = CollectionCheckpoint {
            provider: provider.clone(),
            stream: "completions".into(),
            cursor: "2026-08-19".into(),
            updated_at: instant(19, 1),
        };
        let original = record("same-id", "openai", 17, reported("0.03"));
        storage.insert_usage_record(&original).unwrap();

        let new = record("new-id", "openai", 18, Cost::Unknown);
        let conflict = record("same-id", "openai", 17, reported("0.04"));
        assert!(
            storage
                .insert_usage_records_with_checkpoints(
                    &[new.clone(), conflict],
                    std::slice::from_ref(&checkpoint),
                )
                .is_err()
        );
        assert_eq!(storage.get_usage_record(&new.id).unwrap(), None);
        assert_eq!(
            storage.get_checkpoint(&provider, "completions").unwrap(),
            None
        );

        let summary = storage
            .insert_usage_records_with_checkpoints(
                std::slice::from_ref(&new),
                std::slice::from_ref(&checkpoint),
            )
            .unwrap();
        assert_eq!(summary.inserted, 1);
        assert_eq!(storage.get_usage_record(&new.id).unwrap(), Some(new));
        assert_eq!(
            storage.get_checkpoint(&provider, "completions").unwrap(),
            Some(checkpoint)
        );
    }

    #[test]
    fn backup_is_readable_and_does_not_overwrite() {
        let directory = tempdir().unwrap();
        let backup_path = directory.path().join("backup.db");
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let usage = record("backup", "openai", 17, reported("0.01"));
        storage.insert_usage_record(&usage).unwrap();

        storage.backup_to(&backup_path).unwrap();
        let backup = SqliteStorage::open(&backup_path).unwrap();
        assert_eq!(backup.get_usage_record(&usage.id).unwrap(), Some(usage));
        assert!(matches!(
            storage.backup_to(&backup_path),
            Err(StorageError::BackupDestinationExists(_))
        ));
    }

    #[test]
    fn deletion_cascades_and_full_deletion_clears_checkpoints() {
        let mut storage = SqliteStorage::open_in_memory().unwrap();
        let old = record("old", "openai", 17, reported("0.01"));
        let recent = record("recent", "openai", 19, reported("0.02"));
        storage
            .insert_usage_records(&[old.clone(), recent.clone()])
            .unwrap();
        storage
            .set_checkpoint(&CollectionCheckpoint {
                provider: ProviderId::new("openai").unwrap(),
                stream: "usage".into(),
                cursor: "cursor".into(),
                updated_at: instant(20, 1),
            })
            .unwrap();

        assert_eq!(storage.delete_usage_before(instant(19, 0)).unwrap(), 1);
        assert_eq!(storage.get_usage_record(&old.id).unwrap(), None);
        assert_eq!(storage.row_count("usage_quantities"), 2);

        storage.delete_all_data().unwrap();
        assert_eq!(storage.get_usage_record(&recent.id).unwrap(), None);
        assert_eq!(storage.row_count("collection_checkpoints"), 0);
    }
}

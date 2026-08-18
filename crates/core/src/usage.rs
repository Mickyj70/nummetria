use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors raised while constructing provider-neutral domain values.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("{field} cannot be empty")]
    EmptyIdentifier { field: &'static str },
    #[error("currency must be a three-letter uppercase ISO 4217 code")]
    InvalidCurrency,
    #[error("time range end must be later than its start")]
    InvalidTimeRange,
    #[error("usage quantity cannot be negative")]
    NegativeQuantity,
    #[error("a usage record must contain at least one quantity")]
    MissingQuantities,
    #[error("unsupported usage record schema version {version}")]
    UnsupportedSchemaVersion { version: u16 },
}

macro_rules! identifier {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(DomainError::EmptyIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = DomainError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

identifier!(RecordId, "record id");
identifier!(ProviderId, "provider id");
identifier!(ModelId, "model id");
identifier!(ProjectId, "project id");

/// A validated ISO 4217 currency code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(DomainError::InvalidCurrency);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A half-open UTC interval: start is inclusive and end is exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawTimeRange")]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTimeRange {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl TryFrom<RawTimeRange> for TimeRange {
    type Error = DomainError;

    fn try_from(value: RawTimeRange) -> Result<Self, Self::Error> {
        Self::new(value.start, value.end)
    }
}

impl TimeRange {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, DomainError> {
        if end <= start {
            return Err(DomainError::InvalidTimeRange);
        }
        Ok(Self { start, end })
    }
}

/// A normalized quantity measured by a provider or import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    InputTokens,
    OutputTokens,
    CachedTokens,
    CacheWriteTokens,
    ReasoningTokens,
    Requests,
    Images,
    AudioSeconds,
    VideoSeconds,
    ToolCalls,
    WebSearches,
    ComputeSeconds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawUsageQuantity")]
pub struct UsageQuantity {
    pub kind: UsageKind,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUsageQuantity {
    kind: UsageKind,
    #[serde(with = "rust_decimal::serde::str")]
    amount: Decimal,
}

impl TryFrom<RawUsageQuantity> for UsageQuantity {
    type Error = DomainError;

    fn try_from(value: RawUsageQuantity) -> Result<Self, Self::Error> {
        Self::new(value.kind, value.amount)
    }
}

impl UsageQuantity {
    pub fn new(kind: UsageKind, amount: Decimal) -> Result<Self, DomainError> {
        if amount.is_sign_negative() {
            return Err(DomainError::NegativeQuantity);
        }
        Ok(Self { kind, amount })
    }
}

/// Describes how strongly Nummetria can support a cost value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostEvidence {
    Reported,
    Calculated,
    Estimated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "evidence", deny_unknown_fields)]
pub enum Cost {
    Reported {
        #[serde(with = "rust_decimal::serde::str")]
        amount: Decimal,
        currency: CurrencyCode,
    },
    Calculated {
        #[serde(with = "rust_decimal::serde::str")]
        amount: Decimal,
        currency: CurrencyCode,
        pricing_reference: String,
    },
    Estimated {
        #[serde(with = "rust_decimal::serde::str")]
        amount: Decimal,
        currency: CurrencyCode,
        pricing_reference: String,
    },
    Unknown,
}

impl Cost {
    pub fn evidence(&self) -> CostEvidence {
        match self {
            Self::Reported { .. } => CostEvidence::Reported,
            Self::Calculated { .. } => CostEvidence::Calculated,
            Self::Estimated { .. } => CostEvidence::Estimated,
            Self::Unknown => CostEvidence::Unknown,
        }
    }
}

/// Identifies the path through which a record entered Nummetria.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum CollectionSource {
    ProviderApi { operation: String },
    Import { format: String, source_name: String },
}

/// Version 1 of Nummetria's provider-neutral usage observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsageRecord {
    pub schema_version: u16,
    pub id: RecordId,
    pub provider: ProviderId,
    pub model: Option<ModelId>,
    pub project: Option<ProjectId>,
    pub time_range: TimeRange,
    pub quantities: Vec<UsageQuantity>,
    pub cost: Cost,
    pub source: CollectionSource,
    pub collected_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUsageRecord {
    schema_version: u16,
    id: RecordId,
    provider: ProviderId,
    model: Option<ModelId>,
    project: Option<ProjectId>,
    time_range: TimeRange,
    quantities: Vec<UsageQuantity>,
    cost: Cost,
    source: CollectionSource,
    collected_at: DateTime<Utc>,
}

impl UsageRecord {
    pub const SCHEMA_VERSION: u16 = 1;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RecordId,
        provider: ProviderId,
        model: Option<ModelId>,
        project: Option<ProjectId>,
        time_range: TimeRange,
        quantities: Vec<UsageQuantity>,
        cost: Cost,
        source: CollectionSource,
        collected_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        if quantities.is_empty() {
            return Err(DomainError::MissingQuantities);
        }

        Ok(Self {
            schema_version: Self::SCHEMA_VERSION,
            id,
            provider,
            model,
            project,
            time_range,
            quantities,
            cost,
            source,
            collected_at,
        })
    }
}

impl TryFrom<RawUsageRecord> for UsageRecord {
    type Error = DomainError;

    fn try_from(value: RawUsageRecord) -> Result<Self, Self::Error> {
        if value.schema_version != Self::SCHEMA_VERSION {
            return Err(DomainError::UnsupportedSchemaVersion {
                version: value.schema_version,
            });
        }

        Self::new(
            value.id,
            value.provider,
            value.model,
            value.project,
            value.time_range,
            value.quantities,
            value.cost,
            value.source,
            value.collected_at,
        )
    }
}

impl<'de> Deserialize<'de> for UsageRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawUsageRecord::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::TimeZone;

    use super::*;

    fn example_record() -> UsageRecord {
        UsageRecord::new(
            RecordId::new("openai:usage:2026-08-17:project-a").unwrap(),
            ProviderId::new("openai").unwrap(),
            Some(ModelId::new("gpt-5").unwrap()),
            Some(ProjectId::new("project-a").unwrap()),
            TimeRange::new(
                Utc.with_ymd_and_hms(2026, 8, 17, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 8, 18, 0, 0, 0).unwrap(),
            )
            .unwrap(),
            vec![
                UsageQuantity::new(UsageKind::InputTokens, Decimal::new(1250, 0)).unwrap(),
                UsageQuantity::new(UsageKind::Requests, Decimal::new(4, 0)).unwrap(),
            ],
            Cost::Reported {
                amount: Decimal::from_str("0.03125").unwrap(),
                currency: CurrencyCode::new("USD").unwrap(),
            },
            CollectionSource::ProviderApi {
                operation: "organization_usage".into(),
            },
            Utc.with_ymd_and_hms(2026, 8, 18, 0, 5, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_invalid_domain_values() {
        assert_eq!(
            ProviderId::new("  ").unwrap_err(),
            DomainError::EmptyIdentifier {
                field: "provider id"
            }
        );
        assert_eq!(
            CurrencyCode::new("usd").unwrap_err(),
            DomainError::InvalidCurrency
        );
        assert_eq!(
            UsageQuantity::new(UsageKind::Requests, Decimal::NEGATIVE_ONE).unwrap_err(),
            DomainError::NegativeQuantity
        );
    }

    #[test]
    fn serializes_a_stable_versioned_record() {
        let json = serde_json::to_value(example_record()).unwrap();

        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["provider"], "openai");
        assert_eq!(json["quantities"][0]["amount"], "1250");
        assert_eq!(json["cost"]["amount"], "0.03125");
        assert_eq!(json["cost"]["evidence"], "reported");
    }

    #[test]
    fn round_trips_without_losing_decimal_precision() {
        let record = example_record();
        let json = serde_json::to_string(&record).unwrap();
        let decoded: UsageRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn deserialization_enforces_domain_invariants() {
        let mut json = serde_json::to_value(example_record()).unwrap();
        json["schema_version"] = 2.into();
        assert!(serde_json::from_value::<UsageRecord>(json).is_err());

        let mut json = serde_json::to_value(example_record()).unwrap();
        json["quantities"] = serde_json::json!([]);
        assert!(serde_json::from_value::<UsageRecord>(json).is_err());

        let mut json = serde_json::to_value(example_record()).unwrap();
        json["quantities"][0]["amount"] = "-1".into();
        assert!(serde_json::from_value::<UsageRecord>(json).is_err());
    }

    #[test]
    fn parses_the_public_v1_fixture() {
        let fixture = include_str!("../../../fixtures/usage/valid-v1.json");
        let record: UsageRecord = serde_json::from_str(fixture).unwrap();

        assert_eq!(record.provider.as_str(), "openai");
        assert_eq!(record.cost.evidence(), CostEvidence::Reported);
    }
}

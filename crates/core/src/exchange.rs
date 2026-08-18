use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::UsageRecord;

/// A versioned collection of normalized usage records for import and export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UsageExchange {
    pub format_version: u16,
    pub records: Vec<UsageRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUsageExchange {
    format_version: u16,
    records: Vec<Value>,
}

/// The location and cause of one invalid record in an exchange document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordValidationError {
    pub location: String,
    pub message: String,
}

/// Failures raised while parsing a usage exchange document.
#[derive(Debug, Error)]
pub enum ExchangeError {
    #[error("could not parse the usage exchange: {0}")]
    InvalidEnvelope(#[source] serde_json::Error),
    #[error(
        "unsupported usage exchange format version {found}; this build supports version {supported}"
    )]
    UnsupportedFormat { found: u16, supported: u16 },
    #[error("{} usage record(s) failed validation", .0.len())]
    InvalidRecords(Vec<RecordValidationError>),
}

impl UsageExchange {
    pub const FORMAT_VERSION: u16 = 1;

    pub fn new(records: Vec<UsageRecord>) -> Self {
        Self {
            format_version: Self::FORMAT_VERSION,
            records,
        }
    }

    /// Parses the envelope first, then validates every record independently.
    pub fn from_json_str(input: &str) -> Result<Self, ExchangeError> {
        let raw: RawUsageExchange =
            serde_json::from_str(input).map_err(ExchangeError::InvalidEnvelope)?;

        if raw.format_version != Self::FORMAT_VERSION {
            return Err(ExchangeError::UnsupportedFormat {
                found: raw.format_version,
                supported: Self::FORMAT_VERSION,
            });
        }

        let mut records = Vec::with_capacity(raw.records.len());
        let mut errors = Vec::new();

        for (index, value) in raw.records.into_iter().enumerate() {
            match serde_json::from_value(value) {
                Ok(record) => records.push(record),
                Err(error) => errors.push(RecordValidationError {
                    location: format!("records[{index}]"),
                    message: error.to_string(),
                }),
            }
        }

        if errors.is_empty() {
            Ok(Self::new(records))
        } else {
            Err(ExchangeError::InvalidRecords(errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn valid_record() -> Value {
        serde_json::from_str(include_str!("../../../fixtures/usage/valid-v1.json")).unwrap()
    }

    #[test]
    fn parses_and_round_trips_a_valid_exchange() {
        let input = json!({
            "format_version": 1,
            "records": [valid_record()],
        });

        let exchange = UsageExchange::from_json_str(&input.to_string()).unwrap();
        assert_eq!(exchange.records.len(), 1);

        let output = serde_json::to_string(&exchange).unwrap();
        let reparsed = UsageExchange::from_json_str(&output).unwrap();
        assert_eq!(reparsed, exchange);
    }

    #[test]
    fn accepts_an_empty_exchange() {
        let exchange =
            UsageExchange::from_json_str(r#"{"format_version":1,"records":[]}"#).unwrap();
        assert!(exchange.records.is_empty());
    }

    #[test]
    fn rejects_unknown_envelope_fields() {
        let error =
            UsageExchange::from_json_str(r#"{"format_version":1,"records":[],"unexpected":true}"#)
                .unwrap_err();
        assert!(matches!(error, ExchangeError::InvalidEnvelope(_)));
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_an_unsupported_format_version() {
        let error =
            UsageExchange::from_json_str(r#"{"format_version":2,"records":[]}"#).unwrap_err();
        assert!(matches!(
            error,
            ExchangeError::UnsupportedFormat {
                found: 2,
                supported: 1
            }
        ));
    }

    #[test]
    fn reports_every_invalid_record_by_index() {
        let mut missing_provider = valid_record();
        missing_provider.as_object_mut().unwrap().remove("provider");
        let mut unknown_field = valid_record();
        unknown_field["unexpected"] = true.into();
        let input = json!({
            "format_version": 1,
            "records": [valid_record(), missing_provider, unknown_field],
        });

        let ExchangeError::InvalidRecords(errors) =
            UsageExchange::from_json_str(&input.to_string()).unwrap_err()
        else {
            panic!("expected record validation errors");
        };

        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].location, "records[1]");
        assert_eq!(errors[1].location, "records[2]");
        assert!(errors[0].message.contains("provider"));
        assert!(errors[1].message.contains("unknown field"));
    }
}

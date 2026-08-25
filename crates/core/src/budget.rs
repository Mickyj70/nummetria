use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CurrencyCode, ModelId, ProjectId, ProviderId};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BudgetError {
    #[error("budget name cannot be empty")]
    EmptyName,
    #[error("budget amount must be greater than zero")]
    NonPositiveAmount,
    #[error("budget source filter cannot be empty")]
    EmptySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetPeriod {
    Daily,
    Weekly,
    Monthly,
}

impl BudgetPeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BudgetFilters {
    pub provider: Option<ProviderId>,
    pub model: Option<ModelId>,
    pub project: Option<ProjectId>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub name: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    pub currency: CurrencyCode,
    pub period: BudgetPeriod,
    pub filters: BudgetFilters,
    pub created_at: DateTime<Utc>,
}

impl Budget {
    pub fn new(
        name: impl Into<String>,
        amount: Decimal,
        currency: CurrencyCode,
        period: BudgetPeriod,
        filters: BudgetFilters,
        created_at: DateTime<Utc>,
    ) -> Result<Self, BudgetError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(BudgetError::EmptyName);
        }
        if amount <= Decimal::ZERO {
            return Err(BudgetError::NonPositiveAmount);
        }
        if filters
            .source
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(BudgetError::EmptySource);
        }
        Ok(Self {
            name,
            amount,
            currency,
            period,
            filters,
            created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn validates_budget_invariants() {
        let now = Utc.with_ymd_and_hms(2026, 8, 25, 0, 0, 0).unwrap();
        let valid = Budget::new(
            "monthly-api",
            Decimal::new(25, 0),
            CurrencyCode::new("USD").unwrap(),
            BudgetPeriod::Monthly,
            BudgetFilters {
                provider: Some(ProviderId::new("openai").unwrap()),
                ..BudgetFilters::default()
            },
            now,
        )
        .unwrap();
        assert_eq!(valid.amount.to_string(), "25");
        assert!(matches!(
            Budget::new(
                " ",
                Decimal::ONE,
                CurrencyCode::new("USD").unwrap(),
                BudgetPeriod::Daily,
                BudgetFilters::default(),
                now
            ),
            Err(BudgetError::EmptyName)
        ));
        assert!(matches!(
            Budget::new(
                "zero",
                Decimal::ZERO,
                CurrencyCode::new("USD").unwrap(),
                BudgetPeriod::Daily,
                BudgetFilters::default(),
                now
            ),
            Err(BudgetError::NonPositiveAmount)
        ));
    }
}

//! Provider-neutral domain and application contracts.
//!
//! This crate must not depend on a terminal, database, desktop framework, or a
//! specific AI provider.

/// The product name used by every Nummetria surface.
pub const PRODUCT_NAME: &str = "nummetria";

mod exchange;
mod usage;

pub use exchange::{ExchangeError, RecordValidationError, UsageExchange};
pub use usage::{
    CollectionSource, Cost, CostEvidence, CurrencyCode, DomainError, ModelId, ProjectId,
    ProviderId, RecordId, TimeRange, UsageKind, UsageQuantity, UsageRecord,
};

#[cfg(test)]
mod tests {
    #[test]
    fn product_name_is_stable() {
        assert_eq!(super::PRODUCT_NAME, "nummetria");
    }
}

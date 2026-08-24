use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;

use crate::{
    CollectionSource, Cost, CostEvidence, CurrencyCode, UsageKind, UsageQuantity, UsageRecord,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupDimension {
    None,
    Provider,
    Model,
    Project,
    Source,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportCostTotal {
    pub evidence: CostEvidence,
    pub currency: CurrencyCode,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportGroup {
    pub key: String,
    pub record_count: usize,
    pub quantities: Vec<UsageQuantity>,
    pub costs: Vec<ReportCostTotal>,
    pub unknown_cost_record_count: usize,
}

pub fn aggregate_records(records: &[UsageRecord], dimension: GroupDimension) -> Vec<ReportGroup> {
    let mut groups = BTreeMap::<String, Accumulator>::new();
    for record in records {
        let group = groups.entry(group_key(record, dimension)).or_default();
        group.record_count += 1;
        for quantity in &record.quantities {
            let entry = group
                .quantities
                .entry(usage_key(quantity.kind))
                .or_insert((quantity.kind, Decimal::ZERO));
            entry.1 += quantity.amount;
        }
        match &record.cost {
            Cost::Unknown => group.unknown_cost_record_count += 1,
            Cost::Reported { amount, currency } => {
                group.add_cost(CostEvidence::Reported, currency, *amount)
            }
            Cost::Calculated {
                amount, currency, ..
            } => group.add_cost(CostEvidence::Calculated, currency, *amount),
            Cost::Estimated {
                amount, currency, ..
            } => group.add_cost(CostEvidence::Estimated, currency, *amount),
        }
    }
    groups
        .into_iter()
        .map(|(key, value)| value.finish(key))
        .collect()
}

#[derive(Default)]
struct Accumulator {
    record_count: usize,
    quantities: BTreeMap<&'static str, (UsageKind, Decimal)>,
    costs: BTreeMap<(u8, String), (CostEvidence, CurrencyCode, Decimal)>,
    unknown_cost_record_count: usize,
}

impl Accumulator {
    fn add_cost(&mut self, evidence: CostEvidence, currency: &CurrencyCode, amount: Decimal) {
        let rank = match evidence {
            CostEvidence::Reported => 0,
            CostEvidence::Calculated => 1,
            CostEvidence::Estimated => 2,
            CostEvidence::Unknown => 3,
        };
        let entry = self
            .costs
            .entry((rank, currency.as_str().to_owned()))
            .or_insert((evidence, currency.clone(), Decimal::ZERO));
        entry.2 += amount;
    }

    fn finish(self, key: String) -> ReportGroup {
        ReportGroup {
            key,
            record_count: self.record_count,
            quantities: self
                .quantities
                .into_values()
                .map(|(kind, amount)| UsageQuantity::new(kind, amount).expect("non-negative sum"))
                .collect(),
            costs: self
                .costs
                .into_values()
                .map(|(evidence, currency, amount)| ReportCostTotal {
                    evidence,
                    currency,
                    amount,
                })
                .collect(),
            unknown_cost_record_count: self.unknown_cost_record_count,
        }
    }
}

fn group_key(record: &UsageRecord, dimension: GroupDimension) -> String {
    match dimension {
        GroupDimension::None => "all".into(),
        GroupDimension::Provider => record.provider.as_str().into(),
        GroupDimension::Model => record
            .model
            .as_ref()
            .map_or("unknown", |value| value.as_str())
            .into(),
        GroupDimension::Project => record
            .project
            .as_ref()
            .map_or("unknown", |value| value.as_str())
            .into(),
        GroupDimension::Source => match &record.source {
            CollectionSource::ProviderApi { .. } => "provider_api".into(),
            CollectionSource::LocalTool { tool, .. } => format!("local:{tool}"),
            CollectionSource::Import { .. } => "import".into(),
        },
    }
}

fn usage_key(kind: UsageKind) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProviderId, RecordId, UsageExchange};

    #[test]
    fn groups_exact_values_and_keeps_unknown_cost_visible() {
        let exchange =
            UsageExchange::from_json_str(include_str!("../../../fixtures/exchange/valid-v1.json"))
                .unwrap();
        let mut unknown = exchange.records[0].clone();
        unknown.id = RecordId::new("codex-local-test").unwrap();
        unknown.provider = ProviderId::new("openai-local").unwrap();
        unknown.source = CollectionSource::LocalTool {
            tool: "codex".into(),
            source_id: "session:1".into(),
        };
        unknown.cost = Cost::Unknown;
        let records = [exchange.records[0].clone(), unknown];

        let providers = aggregate_records(&records, GroupDimension::Provider);
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].key, "openai");
        assert_eq!(providers[1].unknown_cost_record_count, 1);

        let sources = aggregate_records(&records, GroupDimension::Source);
        assert_eq!(sources[0].key, "local:codex");
        assert_eq!(sources[1].key, "provider_api");

        let total = aggregate_records(&records, GroupDimension::None);
        assert_eq!(total[0].record_count, 2);
        assert_eq!(total[0].quantities[0].amount, Decimal::from(2500));
        assert_eq!(total[0].costs.len(), 1);
        assert_eq!(total[0].unknown_cost_record_count, 1);
    }
}

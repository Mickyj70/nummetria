use std::str::FromStr;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use nummetria_core::{
    CollectionSource, Cost, CurrencyCode, ModelId, ProjectId, ProviderId, RecordId, TimeRange,
    UsageKind, UsageQuantity, UsageRecord,
};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::{CollectionRange, ProviderBatch, RetryPolicy};

const USAGE_PATH: &str = "/v1/organizations/usage_report/messages";
const COSTS_PATH: &str = "/v1/organizations/cost_report";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Error)]
pub enum AnthropicError {
    #[error("Anthropic rejected the admin credential")]
    Authentication,
    #[error("Anthropic rate limit remained active after retries")]
    RateLimited,
    #[error("Anthropic returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("could not reach Anthropic")]
    Transport,
    #[error("Anthropic returned an invalid {stream} response")]
    InvalidResponse { stream: &'static str },
    #[error("Anthropic indicated another {stream} page without a cursor")]
    MissingPageCursor { stream: &'static str },
    #[error("Anthropic returned an invalid timestamp")]
    InvalidTimestamp,
    #[error("Anthropic returned invalid normalized data: {0}")]
    Domain(String),
}

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    http: Client,
    base_url: String,
    retry: RetryPolicy,
}

impl AnthropicClient {
    pub fn new() -> Result<Self, AnthropicError> {
        Self::with_base_url("https://api.anthropic.com", RetryPolicy::default())
    }

    pub fn with_base_url(
        base_url: impl Into<String>,
        retry: RetryPolicy,
    ) -> Result<Self, AnthropicError> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| AnthropicError::Transport)?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            retry,
        })
    }

    pub fn collect(
        &self,
        admin_key: &str,
        range: &CollectionRange,
        collected_at: DateTime<Utc>,
    ) -> Result<ProviderBatch, AnthropicError> {
        let (mut records, usage_pages) = self.collect_usage(admin_key, range, collected_at)?;
        let (costs, cost_pages) = self.collect_costs(admin_key, range, collected_at)?;
        records.extend(costs);
        Ok(ProviderBatch {
            records,
            usage_pages,
            cost_pages,
        })
    }

    fn collect_usage(
        &self,
        admin_key: &str,
        range: &CollectionRange,
        collected_at: DateTime<Utc>,
    ) -> Result<(Vec<UsageRecord>, usize), AnthropicError> {
        let mut records = Vec::new();
        let mut page = None;
        let mut pages = 0;
        loop {
            let query = query(range, page.as_deref(), &["workspace_id", "model"]);
            let response: ApiPage<UsageResult> =
                self.get(admin_key, USAGE_PATH, &query, "usage")?;
            pages += 1;
            for bucket in response.data {
                for result in bucket.results {
                    records.push(usage_record(
                        &bucket.starting_at,
                        &bucket.ending_at,
                        result,
                        collected_at,
                    )?);
                }
            }
            page = next_page(response.has_more, response.next_page, "usage")?;
            if page.is_none() {
                break;
            }
        }
        Ok((records, pages))
    }

    fn collect_costs(
        &self,
        admin_key: &str,
        range: &CollectionRange,
        collected_at: DateTime<Utc>,
    ) -> Result<(Vec<UsageRecord>, usize), AnthropicError> {
        let mut records = Vec::new();
        let mut page = None;
        let mut pages = 0;
        loop {
            let query = query(range, page.as_deref(), &["workspace_id", "description"]);
            let response: ApiPage<CostResult> = self.get(admin_key, COSTS_PATH, &query, "costs")?;
            pages += 1;
            for bucket in response.data {
                for result in bucket.results {
                    records.push(cost_record(
                        &bucket.starting_at,
                        &bucket.ending_at,
                        result,
                        collected_at,
                    )?);
                }
            }
            page = next_page(response.has_more, response.next_page, "costs")?;
            if page.is_none() {
                break;
            }
        }
        Ok((records, pages))
    }

    fn get<T: for<'de> Deserialize<'de>>(
        &self,
        admin_key: &str,
        path: &str,
        query: &[(String, String)],
        stream: &'static str,
    ) -> Result<T, AnthropicError> {
        let attempts = self.retry.max_attempts.max(1);
        for attempt in 0..attempts {
            let response = self
                .http
                .get(format!("{}{}", self.base_url, path))
                .header("x-api-key", admin_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("Accept", "application/json")
                .query(query)
                .send();
            match response {
                Ok(response) if response.status().is_success() => {
                    return response
                        .json()
                        .map_err(|_| AnthropicError::InvalidResponse { stream });
                }
                Ok(response)
                    if matches!(
                        response.status(),
                        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                    ) =>
                {
                    return Err(AnthropicError::Authentication);
                }
                Ok(response)
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    if attempt + 1 == attempts {
                        return Err(if response.status() == StatusCode::TOO_MANY_REQUESTS {
                            AnthropicError::RateLimited
                        } else {
                            AnthropicError::HttpStatus(response.status().as_u16())
                        });
                    }
                    self.delay(attempt);
                }
                Ok(response) => return Err(status_error(response)),
                Err(_) if attempt + 1 < attempts => self.delay(attempt),
                Err(_) => return Err(AnthropicError::Transport),
            }
        }
        Err(AnthropicError::Transport)
    }

    fn delay(&self, attempt: u8) {
        let factor = 1_u32.checked_shl(u32::from(attempt)).unwrap_or(u32::MAX);
        thread::sleep(self.retry.initial_delay.saturating_mul(factor));
    }
}

fn status_error(response: Response) -> AnthropicError {
    AnthropicError::HttpStatus(response.status().as_u16())
}

fn query(range: &CollectionRange, page: Option<&str>, groups: &[&str]) -> Vec<(String, String)> {
    let mut query = vec![
        ("starting_at".into(), range.start.to_rfc3339()),
        ("ending_at".into(), range.end.to_rfc3339()),
        ("bucket_width".into(), "1d".into()),
        ("limit".into(), "31".into()),
    ];
    query.extend(
        groups
            .iter()
            .map(|group| ("group_by[]".into(), (*group).to_owned())),
    );
    if let Some(page) = page {
        query.push(("page".into(), page.to_owned()));
    }
    query
}

fn next_page(
    has_more: bool,
    next_page: Option<String>,
    stream: &'static str,
) -> Result<Option<String>, AnthropicError> {
    if has_more {
        next_page
            .filter(|cursor| !cursor.is_empty())
            .map(Some)
            .ok_or(AnthropicError::MissingPageCursor { stream })
    } else {
        Ok(None)
    }
}

#[derive(Debug, Deserialize)]
struct ApiPage<T> {
    data: Vec<Bucket<T>>,
    has_more: bool,
    next_page: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Bucket<T> {
    starting_at: String,
    ending_at: String,
    results: Vec<T>,
}

#[derive(Debug, Default, Deserialize)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

#[derive(Debug, Default, Deserialize)]
struct ServerToolUse {
    #[serde(default)]
    web_search_requests: u64,
}

#[derive(Debug, Deserialize)]
struct UsageResult {
    uncached_input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation: CacheCreation,
    #[serde(default)]
    server_tool_use: ServerToolUse,
    workspace_id: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostResult {
    amount: String,
    currency: String,
    workspace_id: Option<String>,
    description: Option<String>,
}

fn usage_record(
    starting_at: &str,
    ending_at: &str,
    result: UsageResult,
    collected_at: DateTime<Utc>,
) -> Result<UsageRecord, AnthropicError> {
    let cache_writes = result
        .cache_creation
        .ephemeral_5m_input_tokens
        .checked_add(result.cache_creation.ephemeral_1h_input_tokens)
        .ok_or(AnthropicError::InvalidResponse { stream: "usage" })?;
    let mut quantities = vec![
        quantity(UsageKind::InputTokens, result.uncached_input_tokens)?,
        quantity(UsageKind::OutputTokens, result.output_tokens)?,
        quantity(UsageKind::CachedTokens, result.cache_read_input_tokens)?,
        quantity(UsageKind::CacheWriteTokens, cache_writes)?,
    ];
    if result.server_tool_use.web_search_requests > 0 {
        quantities.push(quantity(
            UsageKind::WebSearches,
            result.server_tool_use.web_search_requests,
        )?);
    }
    let id = format!(
        "anthropic:messages:{}:{}:{}:{}",
        encode_component(Some(starting_at)),
        encode_component(Some(ending_at)),
        encode_component(result.workspace_id.as_deref()),
        encode_component(result.model.as_deref())
    );
    UsageRecord::new(
        RecordId::new(id).map_err(domain_error)?,
        ProviderId::new("anthropic").map_err(domain_error)?,
        result
            .model
            .map(ModelId::new)
            .transpose()
            .map_err(domain_error)?,
        result
            .workspace_id
            .map(ProjectId::new)
            .transpose()
            .map_err(domain_error)?,
        bucket_range(starting_at, ending_at)?,
        quantities,
        Cost::Unknown,
        CollectionSource::ProviderApi {
            operation: "organization_usage_messages".into(),
        },
        collected_at,
    )
    .map_err(domain_error)
}

fn cost_record(
    starting_at: &str,
    ending_at: &str,
    result: CostResult,
    collected_at: DateTime<Utc>,
) -> Result<UsageRecord, AnthropicError> {
    let amount = Decimal::from_str(&result.amount)
        .map_err(|_| AnthropicError::InvalidResponse { stream: "costs" })?;
    let currency = result.currency.to_ascii_uppercase();
    let id = format!(
        "anthropic:costs:{}:{}:{}:{}:{}",
        encode_component(Some(starting_at)),
        encode_component(Some(ending_at)),
        encode_component(result.workspace_id.as_deref()),
        encode_component(result.description.as_deref()),
        encode_component(Some(&currency))
    );
    let operation = result.description.as_deref().map_or_else(
        || "organization_cost_report".to_owned(),
        |description| format!("organization_cost_report:description={description}"),
    );
    UsageRecord::new(
        RecordId::new(id).map_err(domain_error)?,
        ProviderId::new("anthropic").map_err(domain_error)?,
        None,
        result
            .workspace_id
            .map(ProjectId::new)
            .transpose()
            .map_err(domain_error)?,
        bucket_range(starting_at, ending_at)?,
        Vec::new(),
        Cost::Reported {
            amount,
            currency: CurrencyCode::new(currency).map_err(domain_error)?,
        },
        CollectionSource::ProviderApi { operation },
        collected_at,
    )
    .map_err(domain_error)
}

fn bucket_range(starting_at: &str, ending_at: &str) -> Result<TimeRange, AnthropicError> {
    let start = DateTime::parse_from_rfc3339(starting_at)
        .map_err(|_| AnthropicError::InvalidTimestamp)?
        .with_timezone(&Utc);
    let end = DateTime::parse_from_rfc3339(ending_at)
        .map_err(|_| AnthropicError::InvalidTimestamp)?
        .with_timezone(&Utc);
    TimeRange::new(start, end).map_err(domain_error)
}

fn quantity(kind: UsageKind, value: u64) -> Result<UsageQuantity, AnthropicError> {
    UsageQuantity::new(kind, Decimal::from(value)).map_err(domain_error)
}

fn domain_error(error: impl std::fmt::Display) -> AnthropicError {
    AnthropicError::Domain(error.to_string())
}

fn encode_component(value: Option<&str>) -> String {
    match value {
        None => "-".to_owned(),
        Some(value) => value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use mockito::{Matcher, Server};

    use super::*;

    fn range() -> CollectionRange {
        CollectionRange::new(
            Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 3, 0, 0, 0).unwrap(),
        )
        .unwrap()
    }

    fn client(server: &Server) -> AnthropicClient {
        AnthropicClient::with_base_url(
            server.url(),
            RetryPolicy {
                max_attempts: 1,
                initial_delay: Duration::ZERO,
            },
        )
        .unwrap()
    }

    #[test]
    fn collects_paginated_usage_and_reported_costs() {
        let mut server = Server::new();
        let usage_one = server
            .mock("GET", USAGE_PATH)
            .match_header("x-api-key", "admin-test-key")
            .match_header("anthropic-version", ANTHROPIC_VERSION)
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(include_str!("../fixtures/anthropic-usage-page-1.json"))
            .create();
        let usage_two = server
            .mock("GET", USAGE_PATH)
            .match_query(Matcher::UrlEncoded("page".into(), "usage-page-2".into()))
            .with_status(200)
            .with_body(include_str!("../fixtures/anthropic-usage-page-2.json"))
            .create();
        let costs = server
            .mock("GET", COSTS_PATH)
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(include_str!("../fixtures/anthropic-costs.json"))
            .create();

        let collected = client(&server)
            .collect(
                "admin-test-key",
                &range(),
                Utc.with_ymd_and_hms(2026, 8, 3, 0, 5, 0).unwrap(),
            )
            .unwrap();

        usage_one.assert();
        usage_two.assert();
        costs.assert();
        assert_eq!(collected.usage_pages, 2);
        assert_eq!(collected.cost_pages, 1);
        assert_eq!(collected.records.len(), 3);
        assert_eq!(collected.records[0].provider.as_str(), "anthropic");
        assert_eq!(
            collected.records[0].model.as_ref().unwrap().as_str(),
            "claude-sonnet-4-20250514"
        );
        assert!(matches!(collected.records[0].cost, Cost::Unknown));
        assert!(collected.records[2].quantities.is_empty());
        assert!(matches!(collected.records[2].cost, Cost::Reported { .. }));
        assert!(!format!("{collected:?}").contains("admin-test-key"));
    }

    #[test]
    fn errors_are_sanitized_and_missing_cursors_are_rejected() {
        let mut server = Server::new();
        let auth = server
            .mock("GET", USAGE_PATH)
            .match_query(Matcher::Any)
            .with_status(401)
            .with_body("credential admin-test-key was rejected")
            .create();
        let error = client(&server)
            .collect(
                "admin-test-key",
                &range(),
                Utc.with_ymd_and_hms(2026, 8, 3, 0, 5, 0).unwrap(),
            )
            .unwrap_err();
        auth.assert();
        assert!(matches!(error, AnthropicError::Authentication));
        assert!(!error.to_string().contains("admin-test-key"));

        let missing = next_page(true, None, "usage").unwrap_err();
        assert!(matches!(
            missing,
            AnthropicError::MissingPageCursor { stream: "usage" }
        ));
    }

    #[test]
    fn rate_limits_are_retried_only_up_to_the_policy_limit() {
        let mut server = Server::new();
        let rate_limit = server
            .mock("GET", USAGE_PATH)
            .match_query(Matcher::Any)
            .with_status(429)
            .expect(3)
            .create();
        let client = AnthropicClient::with_base_url(
            server.url(),
            RetryPolicy {
                max_attempts: 3,
                initial_delay: Duration::ZERO,
            },
        )
        .unwrap();

        let error = client
            .collect(
                "admin-test-key",
                &range(),
                Utc.with_ymd_and_hms(2026, 8, 3, 0, 5, 0).unwrap(),
            )
            .unwrap_err();

        rate_limit.assert();
        assert!(matches!(error, AnthropicError::RateLimited));
        assert!(!error.to_string().contains("admin-test-key"));
    }
}

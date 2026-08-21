use std::str::FromStr;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use nummetria_core::{
    CollectionSource, Cost, CurrencyCode, ModelId, ProjectId, ProviderId, RecordId, TimeRange,
    UsageKind, UsageQuantity, UsageRecord,
};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

const COMPLETIONS_PATH: &str = "/v1/organization/usage/completions";
const COSTS_PATH: &str = "/v1/organization/costs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl CollectionRange {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, OpenAiError> {
        if end <= start {
            return Err(OpenAiError::InvalidRange);
        }
        Ok(Self { start, end })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub initial_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(250),
        }
    }
}

#[derive(Debug)]
pub struct ProviderBatch {
    pub records: Vec<UsageRecord>,
    pub usage_pages: usize,
    pub cost_pages: usize,
}

#[derive(Debug, Error)]
pub enum OpenAiError {
    #[error("collection end must be later than collection start")]
    InvalidRange,
    #[error("OpenAI rejected the admin credential")]
    Authentication,
    #[error("OpenAI rate limit remained active after retries")]
    RateLimited,
    #[error("OpenAI returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("could not reach OpenAI")]
    Transport,
    #[error("OpenAI returned an invalid {stream} response")]
    InvalidResponse { stream: &'static str },
    #[error("OpenAI indicated another {stream} page without a cursor")]
    MissingPageCursor { stream: &'static str },
    #[error("OpenAI returned an invalid timestamp")]
    InvalidTimestamp,
    #[error("OpenAI returned invalid normalized data: {0}")]
    Domain(String),
}

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    http: Client,
    base_url: String,
    retry: RetryPolicy,
}

impl OpenAiClient {
    pub fn new() -> Result<Self, OpenAiError> {
        Self::with_base_url("https://api.openai.com", RetryPolicy::default())
    }

    pub fn with_base_url(
        base_url: impl Into<String>,
        retry: RetryPolicy,
    ) -> Result<Self, OpenAiError> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| OpenAiError::Transport)?;
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
    ) -> Result<ProviderBatch, OpenAiError> {
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
    ) -> Result<(Vec<UsageRecord>, usize), OpenAiError> {
        let mut records = Vec::new();
        let mut page = None;
        let mut pages = 0;
        loop {
            let query = common_query(range, page.as_deref(), &["project_id", "model"]);
            let response: ApiPage<CompletionResult> =
                self.get(admin_key, COMPLETIONS_PATH, &query, "usage")?;
            pages += 1;
            for bucket in response.data {
                let start_time = bucket.start_time;
                let end_time = bucket.end_time;
                for result in bucket.results {
                    records.push(usage_record(start_time, end_time, result, collected_at)?);
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
    ) -> Result<(Vec<UsageRecord>, usize), OpenAiError> {
        let mut records = Vec::new();
        let mut page = None;
        let mut pages = 0;
        loop {
            let query = common_query(range, page.as_deref(), &["project_id", "line_item"]);
            let response: ApiPage<CostResult> = self.get(admin_key, COSTS_PATH, &query, "costs")?;
            pages += 1;
            for bucket in response.data {
                let start_time = bucket.start_time;
                let end_time = bucket.end_time;
                for result in bucket.results {
                    records.push(cost_record(start_time, end_time, result, collected_at)?);
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
    ) -> Result<T, OpenAiError> {
        let attempts = self.retry.max_attempts.max(1);
        for attempt in 0..attempts {
            let response = self
                .http
                .get(format!("{}{}", self.base_url, path))
                .bearer_auth(admin_key)
                .header("Accept", "application/json")
                .query(query)
                .send();
            match response {
                Ok(response) if response.status().is_success() => {
                    return response
                        .json()
                        .map_err(|_| OpenAiError::InvalidResponse { stream });
                }
                Ok(response)
                    if matches!(
                        response.status(),
                        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                    ) =>
                {
                    return Err(OpenAiError::Authentication);
                }
                Ok(response)
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    if attempt + 1 == attempts {
                        return Err(if response.status() == StatusCode::TOO_MANY_REQUESTS {
                            OpenAiError::RateLimited
                        } else {
                            OpenAiError::HttpStatus(response.status().as_u16())
                        });
                    }
                    self.delay(attempt);
                }
                Ok(response) => return Err(status_error(response)),
                Err(_) if attempt + 1 < attempts => self.delay(attempt),
                Err(_) => return Err(OpenAiError::Transport),
            }
        }
        Err(OpenAiError::Transport)
    }

    fn delay(&self, attempt: u8) {
        let factor = 1_u32.checked_shl(u32::from(attempt)).unwrap_or(u32::MAX);
        thread::sleep(self.retry.initial_delay.saturating_mul(factor));
    }
}

fn status_error(response: Response) -> OpenAiError {
    OpenAiError::HttpStatus(response.status().as_u16())
}

fn common_query(
    range: &CollectionRange,
    page: Option<&str>,
    groups: &[&str],
) -> Vec<(String, String)> {
    let mut query = vec![
        ("start_time".into(), range.start.timestamp().to_string()),
        ("end_time".into(), range.end.timestamp().to_string()),
        ("bucket_width".into(), "1d".into()),
        ("limit".into(), "31".into()),
    ];
    query.extend(
        groups
            .iter()
            .map(|group| ("group_by".into(), (*group).to_owned())),
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
) -> Result<Option<String>, OpenAiError> {
    if has_more {
        next_page
            .filter(|cursor| !cursor.is_empty())
            .map(Some)
            .ok_or(OpenAiError::MissingPageCursor { stream })
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
    start_time: i64,
    end_time: i64,
    results: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct CompletionResult {
    input_tokens: u64,
    output_tokens: u64,
    num_model_requests: u64,
    #[serde(default)]
    input_cached_tokens: Option<u64>,
    #[serde(default)]
    input_cache_write_tokens: Option<u64>,
    project_id: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CostResult {
    amount: Option<ApiAmount>,
    project_id: Option<String>,
    line_item: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiAmount {
    value: serde_json::Number,
    currency: String,
}

fn usage_record(
    start_time: i64,
    end_time: i64,
    result: CompletionResult,
    collected_at: DateTime<Utc>,
) -> Result<UsageRecord, OpenAiError> {
    let range = bucket_range(start_time, end_time)?;
    let mut quantities = vec![
        quantity(UsageKind::InputTokens, result.input_tokens)?,
        quantity(UsageKind::OutputTokens, result.output_tokens)?,
        quantity(UsageKind::Requests, result.num_model_requests)?,
    ];
    if let Some(value) = result.input_cached_tokens {
        quantities.push(quantity(UsageKind::CachedTokens, value)?);
    }
    if let Some(value) = result.input_cache_write_tokens {
        quantities.push(quantity(UsageKind::CacheWriteTokens, value)?);
    }
    let id = format!(
        "openai:completions:{}:{}:{}:{}",
        start_time,
        end_time,
        encode_component(result.project_id.as_deref()),
        encode_component(result.model.as_deref())
    );
    UsageRecord::new(
        RecordId::new(id).map_err(domain_error)?,
        ProviderId::new("openai").map_err(domain_error)?,
        result
            .model
            .map(ModelId::new)
            .transpose()
            .map_err(domain_error)?,
        result
            .project_id
            .map(ProjectId::new)
            .transpose()
            .map_err(domain_error)?,
        range,
        quantities,
        Cost::Unknown,
        CollectionSource::ProviderApi {
            operation: "organization_usage_completions".into(),
        },
        collected_at,
    )
    .map_err(domain_error)
}

fn cost_record(
    start_time: i64,
    end_time: i64,
    result: CostResult,
    collected_at: DateTime<Utc>,
) -> Result<UsageRecord, OpenAiError> {
    let amount = result
        .amount
        .ok_or(OpenAiError::InvalidResponse { stream: "costs" })?;
    let decimal = Decimal::from_str(&amount.value.to_string())
        .map_err(|_| OpenAiError::InvalidResponse { stream: "costs" })?;
    let currency = amount.currency.to_ascii_uppercase();
    let id = format!(
        "openai:costs:{}:{}:{}:{}:{}",
        start_time,
        end_time,
        encode_component(result.project_id.as_deref()),
        encode_component(result.line_item.as_deref()),
        encode_component(Some(&currency))
    );
    let operation = match result.line_item.as_deref() {
        Some(line_item) => format!("organization_costs:line_item={line_item}"),
        None => "organization_costs".to_owned(),
    };
    UsageRecord::new(
        RecordId::new(id).map_err(domain_error)?,
        ProviderId::new("openai").map_err(domain_error)?,
        None,
        result
            .project_id
            .map(ProjectId::new)
            .transpose()
            .map_err(domain_error)?,
        bucket_range(start_time, end_time)?,
        Vec::new(),
        Cost::Reported {
            amount: decimal,
            currency: CurrencyCode::new(currency).map_err(domain_error)?,
        },
        CollectionSource::ProviderApi { operation },
        collected_at,
    )
    .map_err(domain_error)
}

fn bucket_range(start_time: i64, end_time: i64) -> Result<TimeRange, OpenAiError> {
    let start = Utc
        .timestamp_opt(start_time, 0)
        .single()
        .ok_or(OpenAiError::InvalidTimestamp)?;
    let end = Utc
        .timestamp_opt(end_time, 0)
        .single()
        .ok_or(OpenAiError::InvalidTimestamp)?;
    TimeRange::new(start, end).map_err(domain_error)
}

fn quantity(kind: UsageKind, value: u64) -> Result<UsageQuantity, OpenAiError> {
    UsageQuantity::new(kind, Decimal::from(value)).map_err(domain_error)
}

fn domain_error(error: impl std::fmt::Display) -> OpenAiError {
    OpenAiError::Domain(error.to_string())
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

    fn client(server: &Server) -> OpenAiClient {
        OpenAiClient::with_base_url(
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
        let usage_page_one = server
            .mock("GET", COMPLETIONS_PATH)
            .match_header("authorization", "Bearer admin-test-key")
            .match_query(Matcher::Exact(
                "start_time=1785542400&end_time=1785715200&bucket_width=1d&limit=31&group_by=project_id&group_by=model".into(),
            ))
            .with_status(200)
            .with_body(include_str!("../fixtures/openai-usage-page-1.json"))
            .create();
        let usage_page_two = server
            .mock("GET", COMPLETIONS_PATH)
            .match_query(Matcher::Exact(
                "start_time=1785542400&end_time=1785715200&bucket_width=1d&limit=31&group_by=project_id&group_by=model&page=usage-page-2".into(),
            ))
            .with_status(200)
            .with_body(include_str!("../fixtures/openai-usage-page-2.json"))
            .create();
        let costs = server
            .mock("GET", COSTS_PATH)
            .match_query(Matcher::Exact(
                "start_time=1785542400&end_time=1785715200&bucket_width=1d&limit=31&group_by=project_id&group_by=line_item".into(),
            ))
            .with_status(200)
            .with_body(include_str!("../fixtures/openai-costs.json"))
            .create();

        let collected = client(&server)
            .collect(
                "admin-test-key",
                &range(),
                Utc.with_ymd_and_hms(2026, 8, 3, 0, 5, 0).unwrap(),
            )
            .unwrap();

        usage_page_one.assert();
        usage_page_two.assert();
        costs.assert();
        assert_eq!(collected.usage_pages, 2);
        assert_eq!(collected.cost_pages, 1);
        assert_eq!(collected.records.len(), 3);
        assert_eq!(collected.records[0].provider.as_str(), "openai");
        assert_eq!(
            collected.records[0].model.as_ref().unwrap().as_str(),
            "gpt-5"
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
            .mock("GET", COMPLETIONS_PATH)
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
        assert!(matches!(error, OpenAiError::Authentication));
        assert!(!error.to_string().contains("admin-test-key"));

        let missing = next_page(true, None, "usage").unwrap_err();
        assert!(matches!(
            missing,
            OpenAiError::MissingPageCursor { stream: "usage" }
        ));
    }

    #[test]
    fn rate_limits_are_retried_only_up_to_the_policy_limit() {
        let mut server = Server::new();
        let rate_limit = server
            .mock("GET", COMPLETIONS_PATH)
            .match_query(Matcher::Any)
            .with_status(429)
            .expect(3)
            .create();
        let client = OpenAiClient::with_base_url(
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
        assert!(matches!(error, OpenAiError::RateLimited));
        assert!(!error.to_string().contains("admin-test-key"));
    }
}

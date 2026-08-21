//! Provider adapters that normalize external usage without storing raw payloads.

mod openai;

pub use openai::{CollectionRange, OpenAiClient, OpenAiError, ProviderBatch, RetryPolicy};

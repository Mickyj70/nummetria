//! Provider adapters that normalize external usage without storing raw payloads.

mod anthropic;
mod openai;

pub use anthropic::{AnthropicClient, AnthropicError};
pub use openai::{CollectionRange, OpenAiClient, OpenAiError, ProviderBatch, RetryPolicy};

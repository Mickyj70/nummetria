use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration, Utc};
use nummetria_core::{
    CollectionSource, Cost, ModelId, ProviderId, RecordId, TimeRange, UsageKind, UsageQuantity,
    UsageRecord,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodexSourceError {
    #[error("Codex sessions directory is unavailable")]
    SessionsUnavailable,
    #[error("could not read Codex session data")]
    Io(#[source] std::io::Error),
    #[error("malformed Codex rollout at {file}:{line}")]
    Malformed { file: String, line: usize },
    #[error("invalid normalized Codex usage data")]
    Domain,
}

#[derive(Debug)]
pub struct CodexBatch {
    pub records: Vec<UsageRecord>,
    pub files_scanned: usize,
    pub warnings: Vec<String>,
    pub checkpoint: Option<String>,
    pub files_resumed: usize,
    pub files_reset: usize,
    pub lines_examined: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexCursor {
    version: u16,
    files: BTreeMap<String, FileCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileCursor {
    offset: u64,
    line_number: usize,
    session_id: String,
    model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexSupportStatus {
    NotFound,
    Empty,
    Unsupported,
    Supported,
}

impl CodexSupportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Empty => "empty",
            Self::Unsupported => "unsupported",
            Self::Supported => "supported",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CodexInspection {
    pub status: CodexSupportStatus,
    pub rollout_files: usize,
    pub supported_rollout_files: usize,
    pub usage_events: usize,
    pub warnings: Vec<String>,
}

/// Inspects rollout compatibility without returning or storing usage records.
pub fn inspect(codex_home: &Path) -> Result<CodexInspection, CodexSourceError> {
    let sessions = codex_home.join("sessions");
    if !sessions.is_dir() {
        return Ok(CodexInspection {
            status: CodexSupportStatus::NotFound,
            rollout_files: 0,
            supported_rollout_files: 0,
            usage_events: 0,
            warnings: Vec::new(),
        });
    }
    let mut files = Vec::new();
    find_rollouts(&sessions, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Ok(CodexInspection {
            status: CodexSupportStatus::Empty,
            rollout_files: 0,
            supported_rollout_files: 0,
            usage_events: 0,
            warnings: Vec::new(),
        });
    }
    let mut supported_rollout_files = 0;
    let mut usage_events = 0;
    let mut warnings = Vec::new();
    for path in &files {
        let relative = path
            .strip_prefix(&sessions)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let (supported, events) = inspect_rollout(path, &relative, &mut warnings)?;
        supported_rollout_files += usize::from(supported);
        usage_events += events;
    }
    Ok(CodexInspection {
        status: if usage_events > 0 {
            CodexSupportStatus::Supported
        } else {
            CodexSupportStatus::Unsupported
        },
        rollout_files: files.len(),
        supported_rollout_files,
        usage_events,
        warnings,
    })
}

fn inspect_rollout(
    path: &Path,
    relative: &str,
    warnings: &mut Vec<String>,
) -> Result<(bool, usize), CodexSourceError> {
    let mut reader = BufReader::new(File::open(path).map_err(CodexSourceError::Io)?);
    let mut line = String::new();
    let mut line_number = 0;
    let mut has_session = false;
    let mut usage_events = 0;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).map_err(CodexSourceError::Io)?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        let complete = line.ends_with('\n');
        let parsed: RolloutLine = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) if !complete => {
                warnings.push(format!("ignored incomplete final line in {relative}"));
                break;
            }
            Err(_) => {
                return Err(CodexSourceError::Malformed {
                    file: relative.to_owned(),
                    line: line_number,
                });
            }
        };
        if parsed.kind == "session_meta" && parsed.payload.id.is_some() {
            has_session = true;
        } else if parsed.kind == "event_msg"
            && parsed.payload.kind.as_deref() == Some("token_count")
            && parsed
                .payload
                .info
                .and_then(|info| info.last_token_usage)
                .is_some_and(|usage| !usage.is_empty())
        {
            usage_events += 1;
        }
    }
    Ok((has_session && usage_events > 0, usage_events))
}

#[derive(Debug, Deserialize)]
struct RolloutLine {
    timestamp: Option<DateTime<Utc>>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Payload,
}

#[derive(Debug, Default, Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    model: Option<String>,
    info: Option<TokenInfo>,
}

#[derive(Debug, Deserialize)]
struct TokenInfo {
    last_token_usage: Option<TokenUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct TokenUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
}

pub fn collect(
    codex_home: &Path,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    collected_at: DateTime<Utc>,
) -> Result<CodexBatch, CodexSourceError> {
    collect_incremental(codex_home, start, end, collected_at, None)
}

pub fn collect_incremental(
    codex_home: &Path,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    collected_at: DateTime<Utc>,
    checkpoint: Option<&str>,
) -> Result<CodexBatch, CodexSourceError> {
    let sessions = codex_home.join("sessions");
    if !sessions.is_dir() {
        return Err(CodexSourceError::SessionsUnavailable);
    }
    let mut files = Vec::new();
    find_rollouts(&sessions, &mut files)?;
    files.sort();
    let mut records = Vec::new();
    let mut warnings = Vec::new();
    let incremental = start.is_none() && end.is_none();
    let previous = if incremental {
        checkpoint
            .and_then(|value| match serde_json::from_str::<CodexCursor>(value) {
                Ok(cursor) if cursor.version == 1 => Some(cursor),
                _ => {
                    warnings.push(
                        "ignored an invalid Codex checkpoint and performed a full rescan".into(),
                    );
                    None
                }
            })
            .unwrap_or(CodexCursor {
                version: 1,
                files: BTreeMap::new(),
            })
    } else {
        CodexCursor {
            version: 1,
            files: BTreeMap::new(),
        }
    };
    let mut next = CodexCursor {
        version: 1,
        files: BTreeMap::new(),
    };
    let mut files_resumed = 0;
    let mut files_reset = 0;
    let mut lines_examined = 0;
    for file in &files {
        let relative = file
            .strip_prefix(&sessions)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned();
        let prior = incremental.then(|| previous.files.get(&relative)).flatten();
        let outcome = read_rollout(
            file,
            &sessions,
            start,
            end,
            collected_at,
            &mut records,
            &mut warnings,
            prior,
        )?;
        files_resumed += usize::from(outcome.resumed);
        files_reset += usize::from(outcome.reset);
        lines_examined += outcome.lines_examined;
        if incremental {
            if let Some(cursor) = outcome.cursor {
                next.files.insert(relative, cursor);
            }
        }
    }
    records.sort_by(|left, right| {
        left.time_range
            .start
            .cmp(&right.time_range.start)
            .then_with(|| left.id.as_str().cmp(right.id.as_str()))
    });
    Ok(CodexBatch {
        records,
        files_scanned: files.len(),
        warnings,
        checkpoint: incremental
            .then(|| serde_json::to_string(&next).expect("cursor is serializable")),
        files_resumed,
        files_reset,
        lines_examined,
    })
}

fn find_rollouts(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), CodexSourceError> {
    for entry in fs::read_dir(directory).map_err(CodexSourceError::Io)? {
        let entry = entry.map_err(CodexSourceError::Io)?;
        let kind = entry.file_type().map_err(CodexSourceError::Io)?;
        if kind.is_dir() {
            find_rollouts(&entry.path(), output)?;
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            && entry.file_name().to_string_lossy().starts_with("rollout-")
        {
            output.push(entry.path());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_rollout(
    path: &Path,
    root: &Path,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    collected_at: DateTime<Utc>,
    records: &mut Vec<UsageRecord>,
    warnings: &mut Vec<String>,
    previous: Option<&FileCursor>,
) -> Result<ReadOutcome, CodexSourceError> {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let file = File::open(path).map_err(CodexSourceError::Io)?;
    let length = file.metadata().map_err(CodexSourceError::Io)?.len();
    let header_session = read_header_session(path)?;
    let can_resume = previous.is_some_and(|cursor| {
        cursor.offset <= length && Some(cursor.session_id.as_str()) == header_session.as_deref()
    });
    let reset = previous.is_some() && !can_resume;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut line_number = previous
        .filter(|_| can_resume)
        .map_or(0, |cursor| cursor.line_number);
    let mut session_id = previous
        .filter(|_| can_resume)
        .map(|cursor| cursor.session_id.clone());
    let mut model = previous
        .filter(|_| can_resume)
        .and_then(|cursor| cursor.model.clone());
    let mut offset = 0;
    if let Some(cursor) = previous.filter(|_| can_resume) {
        reader
            .seek(SeekFrom::Start(cursor.offset))
            .map_err(CodexSourceError::Io)?;
        offset = cursor.offset;
    }
    let mut lines_examined = 0;
    loop {
        line.clear();
        let line_start = reader.stream_position().map_err(CodexSourceError::Io)?;
        let bytes = reader.read_line(&mut line).map_err(CodexSourceError::Io)?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        lines_examined += 1;
        let complete = line.ends_with('\n');
        let parsed: RolloutLine = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) if !complete => {
                warnings.push(format!("ignored incomplete final line in {relative}"));
                offset = line_start;
                break;
            }
            Err(_) => {
                return Err(CodexSourceError::Malformed {
                    file: relative,
                    line: line_number,
                });
            }
        };
        offset = reader.stream_position().map_err(CodexSourceError::Io)?;
        if parsed.kind == "session_meta" {
            session_id = parsed.payload.id;
        } else if parsed.kind == "turn_context" {
            model = parsed.payload.model;
        } else if parsed.kind == "event_msg"
            && parsed.payload.kind.as_deref() == Some("token_count")
        {
            if let (Some(timestamp), Some(usage), Some(id)) = (
                parsed.timestamp,
                parsed.payload.info.and_then(|info| info.last_token_usage),
                session_id.as_deref(),
            ) {
                if start.is_some_and(|value| timestamp < value)
                    || end.is_some_and(|value| timestamp >= value)
                    || usage.is_empty()
                {
                    continue;
                }
                records.push(normalize(
                    id,
                    line_number,
                    timestamp,
                    usage,
                    model.as_deref(),
                    collected_at,
                )?);
            }
        }
    }
    Ok(ReadOutcome {
        resumed: can_resume,
        reset,
        lines_examined,
        cursor: session_id.map(|session_id| FileCursor {
            offset,
            line_number,
            session_id,
            model,
        }),
    })
}

struct ReadOutcome {
    resumed: bool,
    reset: bool,
    lines_examined: usize,
    cursor: Option<FileCursor>,
}

fn read_header_session(path: &Path) -> Result<Option<String>, CodexSourceError> {
    let mut reader = BufReader::new(File::open(path).map_err(CodexSourceError::Io)?);
    let mut line = String::new();
    if reader.read_line(&mut line).map_err(CodexSourceError::Io)? == 0 {
        return Ok(None);
    }
    let parsed: RolloutLine = match serde_json::from_str(&line) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    Ok((parsed.kind == "session_meta")
        .then_some(parsed.payload.id)
        .flatten())
}

impl TokenUsage {
    fn is_empty(&self) -> bool {
        self.input_tokens == 0
            && self.cached_input_tokens == 0
            && self.cache_write_input_tokens == 0
            && self.output_tokens == 0
            && self.reasoning_output_tokens == 0
    }
}

fn normalize(
    session: &str,
    ordinal: usize,
    timestamp: DateTime<Utc>,
    usage: TokenUsage,
    model: Option<&str>,
    collected_at: DateTime<Utc>,
) -> Result<UsageRecord, CodexSourceError> {
    let mut quantities = Vec::new();
    for (kind, amount) in [
        (UsageKind::InputTokens, usage.input_tokens),
        (UsageKind::CachedTokens, usage.cached_input_tokens),
        (UsageKind::CacheWriteTokens, usage.cache_write_input_tokens),
        (UsageKind::OutputTokens, usage.output_tokens),
        (UsageKind::ReasoningTokens, usage.reasoning_output_tokens),
        (UsageKind::Requests, 1),
    ] {
        if amount > 0 {
            quantities.push(
                UsageQuantity::new(kind, Decimal::from(amount))
                    .map_err(|_| CodexSourceError::Domain)?,
            );
        }
    }
    let source_id = format!("{session}:{}:{ordinal}", timestamp.timestamp_millis());
    UsageRecord::new(
        RecordId::new(format!("codex:{source_id}")).map_err(|_| CodexSourceError::Domain)?,
        ProviderId::new("openai").map_err(|_| CodexSourceError::Domain)?,
        model
            .map(ModelId::new)
            .transpose()
            .map_err(|_| CodexSourceError::Domain)?,
        None,
        TimeRange::new(timestamp, timestamp + Duration::milliseconds(1))
            .map_err(|_| CodexSourceError::Domain)?,
        quantities,
        Cost::Unknown,
        CollectionSource::LocalTool {
            tool: "codex".into(),
            source_id,
        },
        collected_at,
    )
    .map_err(|_| CodexSourceError::Domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::{fs, fs::OpenOptions, io::Write};
    use tempfile::tempdir;

    #[test]
    fn reads_only_numeric_usage_and_never_leaks_content() {
        let home = tempdir().unwrap();
        let sessions = home.path().join("sessions/2026/08/23");
        fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join("rollout-test.jsonl");
        let mut file = File::create(path).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:00Z","type":"session_meta","payload":{{"id":"session-1"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:01Z","type":"turn_context","payload":{{"model":"gpt-5","user_instructions":"SECRET_CANARY"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:02Z","type":"response_item","payload":{{"content":"SECRET_CANARY"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:03Z","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1,"total_tokens":13}}}}}}}}"#).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 23, 11, 0, 0).unwrap();
        let batch = collect(home.path(), None, None, now).unwrap();
        assert_eq!(batch.records.len(), 1);
        let rendered = format!("{batch:?}");
        assert!(!rendered.contains("SECRET_CANARY"));
        assert!(matches!(batch.records[0].cost, Cost::Unknown));
        let inspection = inspect(home.path()).unwrap();
        assert_eq!(inspection.status, CodexSupportStatus::Supported);
        assert_eq!(inspection.rollout_files, 1);
        assert_eq!(inspection.supported_rollout_files, 1);
        assert_eq!(inspection.usage_events, 1);
        assert!(!format!("{inspection:?}").contains("SECRET_CANARY"));
    }

    #[test]
    fn warns_about_an_incomplete_final_line_without_echoing_it() {
        let home = tempdir().unwrap();
        let sessions = home.path().join("sessions");
        fs::create_dir(&sessions).unwrap();
        let mut file = File::create(sessions.join("rollout-test.jsonl")).unwrap();
        write!(file, "{{\"secret\":\"SECRET_CANARY").unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 23, 11, 0, 0).unwrap();
        let batch = collect(home.path(), None, None, now).unwrap();
        assert_eq!(batch.warnings.len(), 1);
        assert!(!batch.warnings[0].contains("SECRET_CANARY"));
    }

    #[test]
    fn inspection_distinguishes_missing_empty_and_unsupported_sources() {
        let missing = tempdir().unwrap();
        assert_eq!(
            inspect(missing.path()).unwrap().status,
            CodexSupportStatus::NotFound
        );

        let empty = tempdir().unwrap();
        fs::create_dir(empty.path().join("sessions")).unwrap();
        assert_eq!(
            inspect(empty.path()).unwrap().status,
            CodexSupportStatus::Empty
        );

        let unsupported = tempdir().unwrap();
        let sessions = unsupported.path().join("sessions");
        fs::create_dir(&sessions).unwrap();
        let mut file = File::create(sessions.join("rollout-test.jsonl")).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:00Z","type":"session_meta","payload":{{"id":"session-1"}}}}"#).unwrap();
        assert_eq!(
            inspect(unsupported.path()).unwrap().status,
            CodexSupportStatus::Unsupported
        );
    }

    #[test]
    fn incremental_collection_reads_only_appended_complete_lines() {
        let home = tempdir().unwrap();
        let sessions = home.path().join("sessions");
        fs::create_dir(&sessions).unwrap();
        let path = sessions.join("rollout-test.jsonl");
        let mut file = File::create(&path).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:00Z","type":"session_meta","payload":{{"id":"session-1"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:01Z","type":"turn_context","payload":{{"model":"gpt-5"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:02Z","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":10,"output_tokens":3}}}}}}}}"#).unwrap();
        drop(file);
        let now = Utc.with_ymd_and_hms(2026, 8, 23, 11, 0, 0).unwrap();
        let first = collect_incremental(home.path(), None, None, now, None).unwrap();
        assert_eq!(first.records.len(), 1);
        assert_eq!(first.lines_examined, 3);

        let second =
            collect_incremental(home.path(), None, None, now, first.checkpoint.as_deref()).unwrap();
        assert!(second.records.is_empty());
        assert_eq!(second.files_resumed, 1);
        assert_eq!(second.lines_examined, 0);

        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-08-23T10:00:03Z","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":4,"output_tokens":2}}}}}}}}"#).unwrap();
        drop(file);
        let third = collect_incremental(home.path(), None, None, now, second.checkpoint.as_deref())
            .unwrap();
        assert_eq!(third.records.len(), 1);
        assert_eq!(third.files_resumed, 1);
        assert_eq!(third.lines_examined, 1);
    }
}

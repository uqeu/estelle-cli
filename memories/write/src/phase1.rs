use crate::build_stage_one_input_message;
use crate::metrics::MEMORY_PHASE_ONE_E2E_MS;
use crate::metrics::MEMORY_PHASE_ONE_JOBS;
use crate::metrics::MEMORY_PHASE_ONE_OUTPUT;
use crate::metrics::MEMORY_PHASE_ONE_TOKEN_USAGE;
use crate::runtime::MemoryStartupContext;
use crate::runtime::StageOneRequestContext;
use codex_config::types::MemoriesConfig;
use codex_core::Prompt;
use codex_core::RolloutRecorder;
use codex_core::config::Config;
use codex_protocol::error::CodexErr;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::RolloutItem;
use codex_protocol::protocol::TokenUsage;
use codex_rollout::INTERACTIVE_SESSION_SOURCES;
use codex_rollout::should_persist_response_item_for_memories;
// 🔴 **TWO REDACTORS EXISTED AND THE WEAKER ONE GUARDED THE DURABLE PATH.**
//
// This module used `codex_secrets::redact_secrets` — FOUR regexes (`secrets/src/sanitizer.rs:4-11`)
// covering `sk-…`, `AKIA…`, `Bearer …` and a `key|token|secret|password` assignment. Measured
// against twelve synthetic key SHAPES, **ten passed through it unredacted**, including
// `estelle_live_…` — our own key format — and `FIRECRAWL_API_KEY=…`, which is the exact shape of a
// leak this project has already paid to rotate. The assignment rule cannot fire on that one at all:
// `_` is a word character, so `\b` never matches before `API` in `FIRECRAWL_API_KEY`.
//
// `estelle_client::redact_secrets` is the other owner: 231 gitleaks-derived rules with entropy
// gating, allowlists for published examples, a base64 sweep, plus seven legacy shapes
// (`estelle-client/src/auth.rs:83`). It was already the redactor on the hook path
// (`tui/src/top_level.rs:849`).
//
// What made this expensive is WHERE the weak one sat: everything below is either persisted to the
// memory store or uploaded in a prompt. One owner per derived fact, and "is this text safe to
// persist" is one fact.
use estelle_client::redact_secrets as redact_secrets_str;

/// Owned-string adapter, so the call sites below read exactly as they did before.
fn redact_secrets(input: String) -> String {
    redact_secrets_str(&input)
}
use futures::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tracing::info;
use tracing::warn;

struct JobResult {
    outcome: JobOutcome,
    token_usage: Option<TokenUsage>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobOutcome {
    SucceededWithOutput,
    SucceededNoOutput,
    Failed,
}

struct Stats {
    claimed: usize,
    succeeded_with_output: usize,
    succeeded_no_output: usize,
    failed: usize,
    total_token_usage: Option<TokenUsage>,
}

/// Phase 1 model output payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageOneOutput {
    /// Detailed markdown raw memory for a single rollout.
    #[serde(rename = "raw_memory")]
    pub(crate) raw_memory: String,
    /// Compact summary line used for routing and indexing.
    #[serde(rename = "rollout_summary")]
    pub(crate) rollout_summary: String,
    /// Optional slug used to derive rollout summary artifact filenames.
    #[serde(default, rename = "rollout_slug")]
    pub(crate) rollout_slug: Option<String>,
}

/// Runs memory phase 1 in strict step order:
/// 1) claim eligible rollout jobs
/// 2) build one stage-1 request context
/// 3) run stage-1 extraction jobs in parallel
/// 4) emit metrics and logs
pub async fn run(context: Arc<MemoryStartupContext>, config: Arc<Config>) {
    let stage_one_context = build_request_context(context.as_ref(), config.as_ref()).await;
    let _phase_one_e2e_timer = stage_one_context.start_timer(MEMORY_PHASE_ONE_E2E_MS);

    // 1. Claim startup job.
    let Some(claimed_candidates) = claim_startup_jobs(context.as_ref(), &config.memories).await
    else {
        return;
    };
    if claimed_candidates.is_empty() {
        stage_one_context.counter(
            MEMORY_PHASE_ONE_JOBS,
            /*inc*/ 1,
            &[("status", "skipped_no_candidates")],
        );
        return;
    }

    // 3. Run the parallel sampling.
    let outcomes = run_jobs(
        context,
        config,
        claimed_candidates,
        stage_one_context.clone(),
    )
    .await;

    // 4. Metrics and logs.
    let counts = aggregate_stats(outcomes);
    emit_metrics(&stage_one_context, &counts);
    info!(
        "memory stage-1 extraction complete: {} job(s) claimed, {} succeeded ({} with output, {} no output), {} failed",
        counts.claimed,
        counts.succeeded_with_output + counts.succeeded_no_output,
        counts.succeeded_with_output,
        counts.succeeded_no_output,
        counts.failed
    );
}

/// Prune old un-used "dead" raw memories.
pub async fn prune(context: &MemoryStartupContext, config: &Config) {
    if let Some(db) = context.state_db() {
        let max_unused_days = config.memories.max_unused_days;
        match db
            .memories()
            .prune_stage1_outputs_for_retention(max_unused_days, crate::stage_one::PRUNE_BATCH_SIZE)
            .await
        {
            Ok(pruned) => {
                if pruned > 0 {
                    info!(
                        "memory startup pruned {pruned} stale stage-1 output row(s) older than {max_unused_days} days"
                    );
                }
            }
            Err(err) => {
                warn!(
                    "memories db prune_stage1_outputs_for_retention failed during memories startup: {err}"
                );
            }
        }
    }
}

/// JSON schema used to constrain phase-1 model output.
pub fn output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "rollout_summary": { "type": "string" },
            "rollout_slug": { "type": ["string", "null"] },
            "raw_memory": { "type": "string" }
        },
        "required": ["rollout_summary", "rollout_slug", "raw_memory"],
        "additionalProperties": false
    })
}

async fn claim_startup_jobs(
    context: &MemoryStartupContext,
    memories_config: &MemoriesConfig,
) -> Option<Vec<codex_state::Stage1JobClaim>> {
    let Some(state_db) = context.state_db() else {
        // This should not happen.
        warn!("state db unavailable while claiming phase-1 startup jobs; skipping");
        return None;
    };

    let allowed_sources = INTERACTIVE_SESSION_SOURCES
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    match state_db
        .memories()
        .claim_stage1_jobs_for_startup(
            context.thread_id(),
            codex_state::Stage1StartupClaimParams {
                scan_limit: crate::stage_one::THREAD_SCAN_LIMIT,
                max_claimed: memories_config.max_rollouts_per_startup,
                max_age_days: memories_config.max_rollout_age_days,
                min_rollout_idle_hours: memories_config.min_rollout_idle_hours,
                allowed_sources: allowed_sources.as_slice(),
                lease_seconds: crate::stage_one::JOB_LEASE_SECONDS,
            },
        )
        .await
    {
        Ok(claims) => Some(claims),
        Err(err) => {
            warn!(
                "memories db claim_stage1_jobs_for_startup failed during memories startup: {err}"
            );
            None
        }
    }
}

async fn build_request_context(
    context: &MemoryStartupContext,
    config: &Config,
) -> StageOneRequestContext {
    let model_name = config.memories.extract_model.clone().unwrap_or_else(|| {
        context
            .provider()
            .memory_extraction_preferred_model()
            .to_string()
    });
    context
        .stage_one_request_context(config, &model_name, crate::stage_one::REASONING_EFFORT)
        .await
}

async fn run_jobs(
    context: Arc<MemoryStartupContext>,
    config: Arc<Config>,
    claimed_candidates: Vec<codex_state::Stage1JobClaim>,
    stage_one_context: StageOneRequestContext,
) -> Vec<JobResult> {
    futures::stream::iter(claimed_candidates)
        .map(|claim| {
            let context = Arc::clone(&context);
            let config = Arc::clone(&config);
            let stage_one_context = stage_one_context.clone();
            async move {
                job::run(context.as_ref(), config.as_ref(), claim, &stage_one_context).await
            }
        })
        .buffer_unordered(crate::stage_one::CONCURRENCY_LIMIT)
        .collect::<Vec<_>>()
        .await
}

mod job {
    use super::*;

    pub(crate) async fn run(
        context: &MemoryStartupContext,
        config: &Config,
        claim: codex_state::Stage1JobClaim,
        stage_one_context: &StageOneRequestContext,
    ) -> JobResult {
        let claimed_thread = claim.thread;
        let (stage_one_output, token_usage) = match sample(
            context,
            config,
            &claimed_thread.rollout_path,
            &claimed_thread.cwd,
            stage_one_context,
        )
        .await
        {
            Ok(output) => output,
            Err(reason) => {
                result::failed(
                    context,
                    claimed_thread.id,
                    &claim.ownership_token,
                    &reason.to_string(),
                )
                .await;
                return JobResult {
                    outcome: JobOutcome::Failed,
                    token_usage: None,
                };
            }
        };

        if stage_one_output.raw_memory.is_empty() || stage_one_output.rollout_summary.is_empty() {
            return JobResult {
                outcome: result::no_output(context, claimed_thread.id, &claim.ownership_token)
                    .await,
                token_usage,
            };
        }

        JobResult {
            outcome: result::success(
                context,
                claimed_thread.id,
                &claim.ownership_token,
                claimed_thread.updated_at.timestamp(),
                &stage_one_output.raw_memory,
                &stage_one_output.rollout_summary,
                stage_one_output.rollout_slug.as_deref(),
            )
            .await,
            token_usage,
        }
    }

    /// Extract the rollout and perform the actual sampling.
    async fn sample(
        context: &MemoryStartupContext,
        config: &Config,
        rollout_path: &Path,
        rollout_cwd: &Path,
        stage_one_context: &StageOneRequestContext,
    ) -> anyhow::Result<(StageOneOutput, Option<TokenUsage>)> {
        let (rollout_items, _, _) = RolloutRecorder::load_rollout_items(rollout_path).await?;
        let rollout_contents = serialize_filtered_rollout_response_items(&rollout_items)?;

        let mut prompt = Prompt::default();
        prompt.input = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: build_stage_one_input_message(
                    &stage_one_context.model_info,
                    rollout_path,
                    rollout_cwd,
                    &rollout_contents,
                )?,
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }];
        prompt.base_instructions = BaseInstructions {
            text: crate::stage_one::PROMPT.to_string(),
        };
        prompt.output_schema = Some(output_schema());
        prompt.output_schema_strict = true;

        let (result, token_usage) = context
            .stream_stage_one_prompt(config, &prompt, stage_one_context)
            .await?;

        let mut output: StageOneOutput = serde_json::from_str(&result)?;
        output.raw_memory = redact_secrets(output.raw_memory);
        output.rollout_summary = redact_secrets(output.rollout_summary);
        output.rollout_slug = output.rollout_slug.map(redact_secrets);

        Ok((output, token_usage))
    }

    mod result {
        use super::*;

        pub(crate) async fn failed(
            context: &MemoryStartupContext,
            thread_id: codex_protocol::ThreadId,
            ownership_token: &str,
            reason: &str,
        ) {
            tracing::warn!("Phase 1 job failed for thread {thread_id}: {reason}");
            if let Some(state_db) = context.state_db() {
                let _ = state_db
                    .memories()
                    .mark_stage1_job_failed(
                        thread_id,
                        ownership_token,
                        reason,
                        crate::stage_one::JOB_RETRY_DELAY_SECONDS,
                    )
                    .await;
            }
        }

        pub(crate) async fn no_output(
            context: &MemoryStartupContext,
            thread_id: codex_protocol::ThreadId,
            ownership_token: &str,
        ) -> JobOutcome {
            let Some(state_db) = context.state_db() else {
                return JobOutcome::Failed;
            };

            if state_db
                .memories()
                .mark_stage1_job_succeeded_no_output(thread_id, ownership_token)
                .await
                .unwrap_or(false)
            {
                JobOutcome::SucceededNoOutput
            } else {
                JobOutcome::Failed
            }
        }

        pub(crate) async fn success(
            context: &MemoryStartupContext,
            thread_id: codex_protocol::ThreadId,
            ownership_token: &str,
            source_updated_at: i64,
            raw_memory: &str,
            rollout_summary: &str,
            rollout_slug: Option<&str>,
        ) -> JobOutcome {
            let Some(state_db) = context.state_db() else {
                return JobOutcome::Failed;
            };

            if state_db
                .memories()
                .mark_stage1_job_succeeded(
                    thread_id,
                    ownership_token,
                    source_updated_at,
                    raw_memory,
                    rollout_summary,
                    rollout_slug,
                )
                .await
                .unwrap_or(false)
            {
                JobOutcome::SucceededWithOutput
            } else {
                JobOutcome::Failed
            }
        }
    }

    /// Serializes filtered stage-1 memory items for prompt inclusion.
    pub(super) fn serialize_filtered_rollout_response_items(
        items: &[RolloutItem],
    ) -> codex_protocol::error::Result<String> {
        let filtered = items
            .iter()
            .filter_map(|item| match item {
                RolloutItem::ResponseItem(item) => sanitize_response_item_for_memories(item),
                RolloutItem::InterAgentCommunication(communication) => {
                    Some(communication.to_model_input_item())
                }
                RolloutItem::SessionMeta(_)
                | RolloutItem::InterAgentCommunicationMetadata { .. }
                | RolloutItem::Compacted(_)
                | RolloutItem::TurnContext(_)
                | RolloutItem::WorldState(_)
                | RolloutItem::EventMsg(_) => None,
            })
            .collect::<Vec<_>>();
        let serialized = serde_json::to_string(&filtered).map_err(|err| {
            CodexErr::InvalidRequest(format!("failed to serialize rollout memory: {err}"))
        })?;
        Ok(redact_secrets(serialized))
    }

    fn sanitize_response_item_for_memories(item: &ResponseItem) -> Option<ResponseItem> {
        let ResponseItem::Message {
            id,
            role,
            content,
            phase,
            internal_chat_message_metadata_passthrough: metadata,
        } = item
        else {
            return should_persist_response_item_for_memories(item).then(|| item.clone());
        };

        if role == "developer" {
            return None;
        }

        if role != "user" {
            return Some(item.clone());
        }

        let content = content
            .iter()
            .filter(|content_item| !is_memory_excluded_contextual_user_fragment(content_item))
            .cloned()
            .collect::<Vec<_>>();
        if content.is_empty() {
            return None;
        }

        Some(ResponseItem::Message {
            id: id.clone(),
            role: role.clone(),
            content,
            phase: phase.clone(),
            internal_chat_message_metadata_passthrough: metadata.clone(),
        })
    }

    fn is_memory_excluded_contextual_user_fragment(content_item: &ContentItem) -> bool {
        let ContentItem::InputText { text } = content_item else {
            return false;
        };

        matches_marked_fragment(text, "# AGENTS.md instructions", "</INSTRUCTIONS>")
            || matches_marked_fragment(text, "<skill>", "</skill>")
    }

    fn matches_marked_fragment(text: &str, start_marker: &str, end_marker: &str) -> bool {
        let trimmed = text.trim_start();
        let starts_with_marker = trimmed
            .get(..start_marker.len())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(start_marker));
        let trimmed = trimmed.trim_end();
        let ends_with_marker = trimmed
            .get(trimmed.len().saturating_sub(end_marker.len())..)
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(end_marker));
        starts_with_marker && ends_with_marker
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn classifies_memory_excluded_fragments() {
            let cases = [
                (
                    "# AGENTS.md instructions for /tmp\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>",
                    true,
                ),
                (
                    "# AGENTS.md instructions\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>",
                    true,
                ),
                (
                    "<skill>\n<name>demo</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>",
                    true,
                ),
                (
                    "<environment_context>\n<cwd>/tmp</cwd>\n</environment_context>",
                    false,
                ),
                (
                    "<subagent_notification>{\"agent_id\":\"a\",\"status\":\"completed\"}</subagent_notification>",
                    false,
                ),
            ];

            for (text, expected) in cases {
                assert_eq!(
                    is_memory_excluded_contextual_user_fragment(&ContentItem::InputText {
                        text: text.to_string(),
                    }),
                    expected,
                    "{text}",
                );
            }
        }

        #[test]
        fn output_schema_requires_rollout_slug_and_keeps_it_nullable() {
            let schema = output_schema();
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .expect("properties object");
            let required = schema
                .get("required")
                .and_then(Value::as_array)
                .expect("required array");

            let mut required_keys = required
                .iter()
                .map(|key| key.as_str().expect("required key string"))
                .collect::<Vec<_>>();
            required_keys.sort_unstable();

            assert!(
                properties.contains_key("rollout_slug"),
                "schema should declare rollout_slug"
            );

            let rollout_slug_type = properties
                .get("rollout_slug")
                .and_then(Value::as_object)
                .and_then(|entry| entry.get("type"))
                .and_then(Value::as_array)
                .expect("rollout_slug type array");
            let mut rollout_slug_types = rollout_slug_type
                .iter()
                .map(|entry| entry.as_str().expect("type entry string"))
                .collect::<Vec<_>>();
            rollout_slug_types.sort_unstable();

            assert_eq!(
                required_keys,
                vec!["raw_memory", "rollout_slug", "rollout_summary"]
            );
            assert_eq!(rollout_slug_types, vec!["null", "string"]);
        }
    }
}

fn aggregate_stats(outcomes: Vec<JobResult>) -> Stats {
    let claimed = outcomes.len();
    let mut succeeded_with_output = 0;
    let mut succeeded_no_output = 0;
    let mut failed = 0;
    let mut total_token_usage = TokenUsage::default();
    let mut has_token_usage = false;

    for outcome in outcomes {
        match outcome.outcome {
            JobOutcome::SucceededWithOutput => succeeded_with_output += 1,
            JobOutcome::SucceededNoOutput => succeeded_no_output += 1,
            JobOutcome::Failed => failed += 1,
        }

        if let Some(token_usage) = outcome.token_usage {
            total_token_usage.add_assign(&token_usage);
            has_token_usage = true;
        }
    }

    Stats {
        claimed,
        succeeded_with_output,
        succeeded_no_output,
        failed,
        total_token_usage: has_token_usage.then_some(total_token_usage),
    }
}

fn emit_metrics(context: &StageOneRequestContext, counts: &Stats) {
    if counts.claimed > 0 {
        context.counter(
            MEMORY_PHASE_ONE_JOBS,
            counts.claimed as i64,
            &[("status", "claimed")],
        );
    }
    if counts.succeeded_with_output > 0 {
        context.counter(
            MEMORY_PHASE_ONE_JOBS,
            counts.succeeded_with_output as i64,
            &[("status", "succeeded")],
        );
        context.counter(
            MEMORY_PHASE_ONE_OUTPUT,
            counts.succeeded_with_output as i64,
            &[],
        );
    }
    if counts.succeeded_no_output > 0 {
        context.counter(
            MEMORY_PHASE_ONE_JOBS,
            counts.succeeded_no_output as i64,
            &[("status", "succeeded_no_output")],
        );
    }
    if counts.failed > 0 {
        context.counter(
            MEMORY_PHASE_ONE_JOBS,
            counts.failed as i64,
            &[("status", "failed")],
        );
    }
    if let Some(token_usage) = counts.total_token_usage.as_ref() {
        context.histogram(
            MEMORY_PHASE_ONE_TOKEN_USAGE,
            token_usage.total_tokens.max(0),
            &[("token_type", "total")],
        );
        context.histogram(
            MEMORY_PHASE_ONE_TOKEN_USAGE,
            token_usage.input_tokens.max(0),
            &[("token_type", "input")],
        );
        context.histogram(
            MEMORY_PHASE_ONE_TOKEN_USAGE,
            token_usage.cached_input(),
            &[("token_type", "cached_input")],
        );
        context.histogram(
            MEMORY_PHASE_ONE_TOKEN_USAGE,
            token_usage.cache_write_input_tokens.max(0),
            &[("token_type", "cache_write_input")],
        );
        context.histogram(
            MEMORY_PHASE_ONE_TOKEN_USAGE,
            token_usage.output_tokens.max(0),
            &[("token_type", "output")],
        );
        context.histogram(
            MEMORY_PHASE_ONE_TOKEN_USAGE,
            token_usage.reasoning_output_tokens.max(0),
            &[("token_type", "reasoning_output")],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::AgentPath;
    use codex_protocol::protocol::InterAgentCommunication;
    use pretty_assertions::assert_eq;

    #[test]
    fn serializes_memory_rollout_with_agents_removed_but_environment_kept() {
        let mixed_contextual_message = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text:
                        "# AGENTS.md instructions for /tmp\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>"
                            .to_string(),
                },
                ContentItem::InputText {
                    text: "# AGENTS.md instructions\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>"
                        .to_string(),
                },
                ContentItem::InputText {
                    text: "<environment_context>\n<cwd>/tmp</cwd>\n</environment_context>"
                        .to_string(),
                },
            ],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };
        let skill_message = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text:
                    "<skill>\n<name>demo</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>"
                        .to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };
        let subagent_message = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "<subagent_notification>{\"agent_id\":\"a\",\"status\":\"completed\"}</subagent_notification>"
                    .to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };

        let serialized = job::serialize_filtered_rollout_response_items(&[
            RolloutItem::ResponseItem(mixed_contextual_message),
            RolloutItem::ResponseItem(skill_message),
            RolloutItem::ResponseItem(subagent_message.clone()),
        ])
        .expect("serialize");
        let parsed: Vec<ResponseItem> = serde_json::from_str(&serialized).expect("parse");

        assert_eq!(
            parsed,
            vec![
                ResponseItem::Message {
                    id: None,
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText {
                        text: "<environment_context>\n<cwd>/tmp</cwd>\n</environment_context>"
                            .to_string(),
                    }],
                    phase: None,
                    internal_chat_message_metadata_passthrough: None,
                },
                subagent_message,
            ]
        );
    }

    #[test]
    fn serializes_memory_rollout_redacts_secrets_before_prompt_upload() {
        let serialized =
            job::serialize_filtered_rollout_response_items(&[RolloutItem::ResponseItem(
                ResponseItem::FunctionCallOutput {
                    id: None,
                    call_id: "call_123".to_string(),
                    output: codex_protocol::models::FunctionCallOutputPayload {
                        body: codex_protocol::models::FunctionCallOutputBody::Text(
                            r#"{"token":"sk-abcdefghijklmnopqrstuvwxyz123456"}"#.to_string(),
                        ),
                        success: Some(true),
                    },
                    internal_chat_message_metadata_passthrough: None,
                },
            )])
            .expect("serialize");

        // Assert the SECRET IS GONE, not which marker replaced it. This pinned
        // `[REDACTED_SECRET]` — the four-regex engine's marker — which made the marker, rather than
        // the absence, the thing under test. A redactor swap is not a regression; a surviving
        // secret is.
        assert!(!serialized.contains("sk-abcdefghijklmnopqrstuvwxyz123456"));
        assert!(
            serialized.to_lowercase().contains("redacted"),
            "something must mark the removal, got: {serialized}"
        );
    }

    /// 🔴 **TEN OF TWELVE KEY SHAPES USED TO REACH THE MEMORY STORE IN CLEARTEXT.**
    ///
    /// Every string below is a synthetic SHAPE — repeated filler characters, never a real
    /// credential. Measured against the four regexes this module used to call
    /// (`secrets/src/sanitizer.rs:4-11`), ten of these twelve passed through untouched: only the
    /// bare `sk-…` and `AKIA…` forms were caught. The two that matter most for us are in that ten:
    /// `estelle_live_…` is our own key format, and `FIRECRAWL_API_KEY=…` is the exact shape of a
    /// leak this project has already paid to rotate — its assignment rule cannot fire, because `_`
    /// is a word character so `\b` never matches before `API`.
    ///
    /// ⚠️ **Limit, stated:** this proves these twelve shapes are caught. It is not a proof that the
    /// engine catches every secret — no redactor can claim that. It is a ratchet on the shapes we
    /// have actually been burned by, and it fails closed when one of them regresses.
    #[test]
    fn key_shapes_we_have_been_burned_by_never_reach_the_memory_store() {
        // ⚠️ **THE FIXTURE IS PART OF THE INSTRUMENT, AND IT WAS THE DEFECT ONCE ALREADY.**
        //
        // The first version of this test filled every shape with a repeated character (`"g"*35`).
        // Shannon entropy of that is ZERO bits/char, and the engine gates per rule — `gcp-api-key`
        // needs 4, `jwt` 3, `npm-access-token` 2 — so four shapes "survived" and the test read as
        // an engine defect when it was a fixture defect. A fixture that cannot clear the gate it is
        // aimed at proves nothing about the rule behind it.
        //
        // So the filler is DERIVED — deterministic, seeded, and drawn from the alphabet each rule
        // actually accepts, giving ~5.9 bits/char over a 62-character set. It is reproducible and
        // contains no real credential.
        //
        // ⚠️ It took THREE goes to make these fixtures honest, and every failure looked identical
        // from the outside — a shape "surviving" redaction: (1) zero-entropy filler that no
        // entropy-gated rule can match, (2) the wrong alphabet for the rule's character class, and
        // (3) a structurally wrong token — a JWT whose payload segment did not start with `ey`,
        // which the rule requires on both segments. Each time the engine was right and the
        // instrument was wrong. **A fixture that cannot trip the rule it is aimed at is
        // indistinguishable from a rule that does not fire**, so a "leak found" from a synthetic
        // corpus has to be confirmed against the rule's own pattern before it is believed.
        fn synthetic_filler(seed: &str, len: usize, alphabet: &[u8]) -> String {
            // FNV-1a, walked one byte at a time. Deterministic, dependency-free, and near-uniform
            // over the alphabet, which is all the entropy gate is measuring.
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in seed.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            (0..len)
                .map(|_| {
                    hash ^= hash >> 33;
                    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
                    hash ^= hash >> 29;
                    char::from(alphabet[(hash % alphabet[..].len() as u64) as usize])
                })
                .collect()
        }

        const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        const B64URL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

        let shapes: Vec<(&str, String)> = vec![
            (
                "anthropic",
                format!("sk-ant-api03-{}", synthetic_filler("anthropic", 40, ALNUM)),
            ),
            (
                "github_pat",
                format!("ghp_{}", synthetic_filler("github", 36, ALNUM)),
            ),
            (
                "stripe_live",
                format!("sk_live_{}", synthetic_filler("stripe", 24, ALNUM)),
            ),
            (
                "slack_bot",
                format!(
                    "xoxb-1111111111-2222222222-{}",
                    synthetic_filler("slack", 24, ALNUM)
                ),
            ),
            (
                "estelle_live",
                format!("estelle_live_{}", synthetic_filler("estelle", 32, ALNUM)),
            ),
            (
                "jwt",
                // Both the header AND the payload segment start with `ey` in a real JWT — that is
                // base64 of `{"`, and the rule requires it on both. A random middle segment does
                // not match, and that was the third way this fixture was wrong.
                format!(
                    "eyJhbGciOiJIUzI1NiJ9.ey{}.{}",
                    synthetic_filler("jwtbody", 38, B64URL),
                    synthetic_filler("jwtsig", 43, B64URL)
                ),
            ),
            (
                "google_api",
                format!("AIza{}", synthetic_filler("google", 35, B64URL)),
            ),
            (
                "npm_token",
                format!("npm_{}", synthetic_filler("npm", 36, ALNUM)),
            ),
            (
                // The exact shape of the leak this project has already paid to rotate: a value
                // under a secret-y NAME. `codex_secrets`' assignment rule cannot fire on it at all,
                // because `_` is a word character so `\b` never matches before `API`.
                "env_line_firecrawl",
                format!(
                    "FIRECRAWL_API_KEY={}",
                    synthetic_filler("firecrawl", 32, ALNUM)
                ),
            ),
            (
                "openai_sk",
                format!("sk-{}", synthetic_filler("openai", 40, ALNUM)),
            ),
            (
                "aws_akia",
                format!(
                    "AKIA{}",
                    synthetic_filler("aws", 16, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
                ),
            ),
        ];

        let mut survivors = Vec::new();
        for (label, shape) in &shapes {
            let serialized =
                job::serialize_filtered_rollout_response_items(&[RolloutItem::ResponseItem(
                    ResponseItem::FunctionCallOutput {
                        id: None,
                        call_id: "call_secret".to_string(),
                        output: codex_protocol::models::FunctionCallOutputPayload {
                            body: codex_protocol::models::FunctionCallOutputBody::Text(
                                shape.clone(),
                            ),
                            success: Some(true),
                        },
                        internal_chat_message_metadata_passthrough: None,
                    },
                )])
                .expect("serialize");

            if serialized.contains(shape.as_str()) {
                survivors.push(*label);
            }
        }

        // Vacuity guard: if the serializer stopped emitting the payload at all, every shape would
        // "pass" while proving nothing about redaction.
        let control = job::serialize_filtered_rollout_response_items(&[RolloutItem::ResponseItem(
            ResponseItem::FunctionCallOutput {
                id: None,
                call_id: "call_control".to_string(),
                output: codex_protocol::models::FunctionCallOutputPayload {
                    body: codex_protocol::models::FunctionCallOutputBody::Text(
                        "ordinary prose that is not a credential".to_string(),
                    ),
                    success: Some(true),
                },
                internal_chat_message_metadata_passthrough: None,
            },
        )])
        .expect("serialize control");
        assert!(
            control.contains("ordinary prose that is not a credential"),
            "the serializer is dropping payloads, so the absence of secrets above proves nothing"
        );

        assert!(
            survivors.is_empty(),
            "these key shapes reached the memory store in cleartext: {survivors:?}"
        );
    }

    #[test]
    fn serializes_inter_agent_communications_for_memory() {
        let plaintext = InterAgentCommunication::new(
            AgentPath::root().join("worker").expect("worker path"),
            AgentPath::root(),
            Vec::new(),
            "child done".to_string(),
            /*trigger_turn*/ false,
        );
        let encrypted = InterAgentCommunication::new_encrypted(
            AgentPath::root(),
            AgentPath::root().join("worker").expect("worker path"),
            Vec::new(),
            "encrypted payload".to_string(),
            /*trigger_turn*/ true,
        );
        let expected = vec![
            plaintext.to_model_input_item(),
            encrypted.to_model_input_item(),
        ];

        let serialized = job::serialize_filtered_rollout_response_items(&[
            RolloutItem::InterAgentCommunication(plaintext),
            RolloutItem::InterAgentCommunication(encrypted),
        ])
        .expect("serialize");
        let parsed: Vec<ResponseItem> = serde_json::from_str(&serialized).expect("parse");

        assert_eq!(parsed, expected);
    }

    #[test]
    fn serializes_agent_message_response_items_for_memory() {
        let communication = InterAgentCommunication::new(
            AgentPath::root(),
            AgentPath::root().join("worker").expect("agent path"),
            Vec::new(),
            "delegated task".to_string(),
            /*trigger_turn*/ true,
        );
        let response_item = communication.to_model_input_item();

        let serialized = job::serialize_filtered_rollout_response_items(&[
            RolloutItem::InterAgentCommunicationMetadata { trigger_turn: true },
            RolloutItem::ResponseItem(response_item.clone()),
        ])
        .expect("serialize");
        let parsed: Vec<ResponseItem> = serde_json::from_str(&serialized).expect("parse");

        assert_eq!(parsed, vec![response_item]);
    }

    #[test]
    fn count_outcomes_sums_token_usage_across_all_jobs() {
        let counts = aggregate_stats(vec![
            JobResult {
                outcome: JobOutcome::SucceededWithOutput,
                token_usage: Some(TokenUsage {
                    input_tokens: 10,
                    cached_input_tokens: 2,
                    cache_write_input_tokens: 0,
                    output_tokens: 3,
                    reasoning_output_tokens: 1,
                    total_tokens: 13,
                }),
            },
            JobResult {
                outcome: JobOutcome::SucceededNoOutput,
                token_usage: Some(TokenUsage {
                    input_tokens: 7,
                    cached_input_tokens: 1,
                    cache_write_input_tokens: 0,
                    output_tokens: 2,
                    reasoning_output_tokens: 0,
                    total_tokens: 9,
                }),
            },
            JobResult {
                outcome: JobOutcome::Failed,
                token_usage: None,
            },
        ]);

        assert_eq!(counts.claimed, 3);
        assert_eq!(counts.succeeded_with_output, 1);
        assert_eq!(counts.succeeded_no_output, 1);
        assert_eq!(counts.failed, 1);
        assert_eq!(
            counts.total_token_usage,
            Some(TokenUsage {
                input_tokens: 17,
                cached_input_tokens: 3,
                cache_write_input_tokens: 0,
                output_tokens: 5,
                reasoning_output_tokens: 1,
                total_tokens: 22,
            })
        );
    }

    #[test]
    fn count_outcomes_keeps_usage_empty_when_no_job_reports_it() {
        let counts = aggregate_stats(vec![
            JobResult {
                outcome: JobOutcome::SucceededWithOutput,
                token_usage: None,
            },
            JobResult {
                outcome: JobOutcome::Failed,
                token_usage: None,
            },
        ]);

        assert_eq!(counts.claimed, 2);
        assert_eq!(counts.total_token_usage, None);
    }
}

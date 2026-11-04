//! Deterministic destination-native compaction boundaries (session-interchange spec §4.4).
//!
//! The whole generated session stays intact. The boundary is planned against the writer's final
//! native items, so its backwards pointer names a real generated item, never a source id or a later
//! one. Each writer renders the plan in its own harness record.

use serde_json::Value;

/// The Gateway's text-token multiplier, kept as its exact IEEE-754 bits so the budget type stays `Eq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenFactor(u64);

impl TokenFactor {
    pub fn new(factor: f64) -> Result<Self, String> {
        if factor.is_finite() && factor > 0.0 {
            Ok(Self(factor.to_bits()))
        } else {
            Err(format!("{factor}: not a finite positive token factor"))
        }
    }

    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// Destination-model facts supplied by the caller. No model is called and no window is guessed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBudget {
    pub context: u64,
    pub max_output_tokens: Option<u64>,
    pub text_token_factor: Option<TokenFactor>,
}

/// One final native item a boundary can be planned against: the writer's own generated row/message,
/// its destination identifier when the format has one, and its user text for the summary.
pub struct NativeItem<'a> {
    pub id: Option<&'a str>,
    pub json: &'a Value,
    pub user_text: Option<&'a str>,
}

/// What a writer appends and what `document` reports to its caller.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeCompaction {
    pub summary: String,
    /// Index of the first retained item in the sequence the boundary was planned against.
    pub start: usize,
    /// The destination's own backwards pointer when the format has one (OpenCode `keepID`,
    /// Pi `firstKeptEntryId`). Codex carries the retained tail explicitly instead.
    pub keep_id: Option<String>,
    pub estimated_tokens: u64,
    pub projected_tokens: u64,
    pub summarized_messages: usize,
    pub summarized_user_messages: usize,
    pub budget: u64,
    pub fits: bool,
}

const RESERVE_SHARE_NUM: u64 = 15;
const RESERVE_MIN: u64 = 16_000;
const NEW_TURN_MARGIN: u64 = 8_000;
const DEFAULT_OUTPUT: u64 = 16_000;
const SUMMARY_HEADER: &str = "[mukade native compaction] Deterministic summary of earlier user requests; the full history remains in this session.";
const USER_MESSAGE_LIMIT: usize = 1_000;
const SUMMARY_BODY_LIMIT: usize = 24_000;

/// Conservative text estimate: chars/3, then the Gateway's multiplier when it is valid (>= 1).
pub fn estimate_tokens(text: &str, text_token_factor: Option<TokenFactor>) -> u64 {
    let chars = text.chars().count() as u64;
    if chars == 0 {
        return 0;
    }
    let base = chars.div_ceil(3);
    let factor = text_token_factor.map(TokenFactor::value).filter(|factor| *factor >= 1.0).unwrap_or(1.0);
    (base as f64 * factor).ceil() as u64
}

/// OpenCode's automatic compaction ceiling: window minus max(15%, 16k for windows >= 32k).
pub fn native_compaction_ceiling(window: u64) -> u64 {
    let reserve = (window * RESERVE_SHARE_NUM / 100).max(if window >= 2 * RESERVE_MIN { RESERVE_MIN } else { 0 });
    window.saturating_sub(reserve)
}

/// A conservative common budget: never above half the window, and below OpenCode's verified native
/// trigger with room for the next turn. The other harnesses' own thresholds are not all verified, so
/// this is deliberately lower rather than destination-specific.
pub fn import_budget(limits: &NativeBudget) -> u64 {
    let room = limits.max_output_tokens.unwrap_or(DEFAULT_OUTPUT) + NEW_TURN_MARGIN;
    (limits.context / 2).min(native_compaction_ceiling(limits.context).saturating_sub(room))
}

fn message_tokens(message: &Value, limits: &NativeBudget) -> u64 {
    estimate_tokens(&message.to_string(), limits.text_token_factor)
}

fn summary_reserve(limits: &NativeBudget) -> u64 {
    estimate_tokens(&format!("{SUMMARY_HEADER}{}", "x".repeat(SUMMARY_BODY_LIMIT + 256)), limits.text_token_factor)
}

/// Index of the first retained message, or None when all fit or no earlier message can be summarized.
fn select_start(weights: &[u64], summary_tokens: u64, budget: u64) -> Option<usize> {
    if weights.len() < 2 {
        return None;
    }
    let available = budget.saturating_sub(summary_tokens);
    let mut tail_tokens = 0_u64;
    let mut start = weights.len();
    for index in (0..weights.len()).rev() {
        let tokens = weights[index];
        if index < weights.len() - 1 && tail_tokens.saturating_add(tokens) > available {
            break;
        }
        tail_tokens = tail_tokens.saturating_add(tokens);
        start = index;
        if tail_tokens >= available {
            break;
        }
    }
    (start > 0).then_some(start)
}

fn clip(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= limit {
        trimmed.to_string()
    } else {
        format!("{}…", chars[..limit - 1].iter().collect::<String>())
    }
}

fn deterministic_summary(users: &[&str]) -> (String, usize) {
    let mut lines: Vec<String> = Vec::new();
    let mut length = 0_usize;
    let mut included = 0_usize;
    for (index, text) in users.iter().enumerate() {
        let line = format!("{}. {}", index + 1, clip(text, USER_MESSAGE_LIMIT));
        let line_chars = line.chars().count();
        if length + line_chars + 1 > SUMMARY_BODY_LIMIT {
            break;
        }
        length += line_chars + 1;
        included += 1;
        lines.push(line);
    }
    if included < users.len() {
        lines.push(format!("… {} earlier user requests omitted from this summary; they remain in the imported history.", users.len() - included));
    }
    if users.is_empty() {
        lines.push("No user messages in the superseded history; its full contents remain in the imported history.".into());
    }
    (format!("{SUMMARY_HEADER}\n{}", lines.join("\n")), users.len())
}

/// Plan the boundary against the writer's final native items. No source message is removed; each
/// writer renders the returned plan in its own native record.
pub fn plan_native_items(items: &[NativeItem<'_>], limits: &NativeBudget) -> Option<NativeCompaction> {
    let budget = import_budget(limits);
    let weights: Vec<u64> = items.iter().map(|item| message_tokens(item.json, limits)).collect();
    let estimated_tokens = weights.iter().sum();
    if estimated_tokens <= budget {
        return None;
    }
    let start = select_start(&weights, summary_reserve(limits), budget)?;
    let users: Vec<&str> = items[..start].iter().filter_map(|item| item.user_text).collect();
    let (summary, summarized_user_messages) = deterministic_summary(&users);
    let tail_tokens: u64 = weights[start..].iter().sum();
    let projected_tokens = estimate_tokens(&summary, limits.text_token_factor).saturating_add(tail_tokens);
    Some(NativeCompaction {
        summary,
        start,
        keep_id: items[start].id.map(str::to_string),
        estimated_tokens,
        projected_tokens,
        summarized_messages: start,
        summarized_user_messages,
        budget,
        fits: projected_tokens <= budget,
    })
}

/// Plan the boundary against final OpenCode messages. The caller appends the returned summary as a
/// native `compaction` record with `metadata.keepID = keep_id`.
pub fn plan_native_compaction(messages: &[Value], limits: &NativeBudget) -> Option<NativeCompaction> {
    let items: Vec<NativeItem> = messages
        .iter()
        .map(|message| NativeItem {
            id: message["id"].as_str(),
            json: message,
            user_text: (message["type"] == "user").then(|| message["text"].as_str()).flatten(),
        })
        .collect();
    plan_native_items(&items, limits)
}

#[cfg(test)]
mod tests {
    use super::select_start;

    #[test]
    fn the_summary_cost_shortens_the_tail_and_the_newest_message_always_survives() {
        assert_eq!(select_start(&[100; 6], 50, 350), Some(3));
        assert_eq!(select_start(&[100; 6], 250, 350), Some(5));
        assert_eq!(select_start(&[50, 500], 20, 100), Some(1));
        assert_eq!(select_start(&[100; 3], 10, 1_000), None);
        assert_eq!(select_start(&[500], 20, 100), None);
    }
}

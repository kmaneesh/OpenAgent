//! Heuristic turn classifier.
//!
//! Decides whether a step should use the fast or strong LLM provider.
//! Called only when `fast_provider` is configured in `openagent.yaml` — otherwise
//! the classifier output is ignored and all turns use the main (strong) provider.
//!
//! # Routing rules (evaluated in order, first match wins)
//!
//! 1. `turn_kind == "tool_call"` → **Fast** — responding to a tool result is
//!    execution/formatting, not reasoning.
//! 2. Active research context present → **Strong** — supervisor must reason about
//!    task selection and possible worker dispatch.
//! 3. Input is ≤ 8 words → **Fast** — short conversational turns need no heavy model.
//! 4. Input contains a complexity signal keyword → **Strong**.
//! 5. Default → **Strong** (err on the side of quality).

/// Which LLM provider tier to use for this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTier {
    /// Use the fast provider (short/simple turns, tool_call turns).
    Fast,
    /// Use the strong provider (research, reasoning, analysis).
    Strong,
}

/// Classify a turn into a provider tier.
///
/// - `user_input`           — raw user message
/// - `has_research_context` — true when `fetch_research_context` returned Some
/// - `turn_kind`            — `"generation"` or `"tool_call"`
pub fn classify(user_input: &str, has_research_context: bool, turn_kind: &str) -> ProviderTier {
    // Rule 1: tool_call turns are execution, not reasoning.
    if turn_kind == "tool_call" {
        return ProviderTier::Fast;
    }

    // Rule 2: active research → supervisor must reason carefully.
    if has_research_context {
        return ProviderTier::Strong;
    }

    // Rule 3: complexity signal keywords — checked before word count so a short
    // but complex command ("please analyse this") still routes to strong.
    const COMPLEX_SIGNALS: &[&str] = &[
        "research", "analyse", "analyze", "summarise", "summarize",
        "compare", "explain", "investigate", "synthesize", "synthesise",
        "write", "generate", "plan", "strategy", "hypothesis",
        "review", "evaluate", "assess", "contradict", "verify",
    ];
    let trimmed = user_input.trim();
    let lower = trimmed.to_lowercase();
    for signal in COMPLEX_SIGNALS {
        if lower.contains(signal) {
            return ProviderTier::Strong;
        }
    }

    // Rule 4: very short input with no complexity signal — likely conversational.
    let word_count = trimmed.split_whitespace().count();
    if word_count <= 8 {
        return ProviderTier::Fast;
    }

    // Default: strong.
    ProviderTier::Strong
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_turn_is_always_fast() {
        assert_eq!(
            classify("anything here", false, "tool_call"),
            ProviderTier::Fast
        );
        assert_eq!(
            classify("complex research analysis task", true, "tool_call"),
            ProviderTier::Fast
        );
    }

    #[test]
    fn active_research_forces_strong() {
        assert_eq!(
            classify("what is next?", true, "generation"),
            ProviderTier::Strong
        );
    }

    #[test]
    fn short_input_no_research_is_fast() {
        assert_eq!(classify("hello", false, "generation"), ProviderTier::Fast);
        assert_eq!(classify("what time is it", false, "generation"), ProviderTier::Fast);
        // exactly 8 words
        assert_eq!(
            classify("one two three four five six seven eight", false, "generation"),
            ProviderTier::Fast
        );
    }

    #[test]
    fn nine_words_with_no_signal_is_strong() {
        assert_eq!(
            classify("one two three four five six seven eight nine", false, "generation"),
            ProviderTier::Strong
        );
    }

    #[test]
    fn complexity_signal_forces_strong_even_if_short() {
        assert_eq!(
            classify("please analyse this", false, "generation"),
            ProviderTier::Strong
        );
        assert_eq!(
            classify("write a plan", false, "generation"),
            ProviderTier::Strong
        );
    }

    #[test]
    fn long_input_no_signal_is_strong_by_default() {
        // 9 words, no complexity signal → falls through to default Strong
        assert_eq!(
            classify("what is the current news about the space mission", false, "generation"),
            ProviderTier::Strong
        );
    }

    #[test]
    fn short_conversational_no_signal_is_fast() {
        // 7 words, no signal → Fast
        assert_eq!(
            classify("tell me about the weather today", false, "generation"),
            ProviderTier::Fast
        );
    }
}

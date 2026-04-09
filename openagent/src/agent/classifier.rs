//! Heuristic turn classifier.
//!
//! Decides whether a step should use the fast or strong LLM provider.
//! Called only when `fast_provider` is configured in `openagent.toml` — otherwise
//! the classifier output is ignored and all turns use the main (strong) provider.
//!
//! # Routing rules (evaluated in order, first match wins)
//!
//! 1. Input contains a complexity signal keyword → **Strong**.
//! 2. Input is ≤ 8 words → **Fast** — short conversational turns need no heavy model.
//! 3. Default → **Strong** (err on the side of quality).

/// Which LLM provider tier to use for this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTier {
    /// Use the fast provider (short/simple turns).
    Fast,
    /// Use the strong provider (reasoning, analysis, complex tasks).
    Strong,
}

/// Classify a turn into a provider tier based on input complexity.
pub fn classify(user_input: &str) -> ProviderTier {
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

    let word_count = trimmed.split_whitespace().count();
    if word_count <= 8 {
        return ProviderTier::Fast;
    }

    ProviderTier::Strong
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_input_no_signal_is_fast() {
        assert_eq!(classify("hello"), ProviderTier::Fast);
        assert_eq!(classify("what time is it"), ProviderTier::Fast);
        assert_eq!(classify("one two three four five six seven eight"), ProviderTier::Fast);
    }

    #[test]
    fn nine_words_with_no_signal_is_strong() {
        assert_eq!(
            classify("one two three four five six seven eight nine"),
            ProviderTier::Strong
        );
    }

    #[test]
    fn complexity_signal_forces_strong_even_if_short() {
        assert_eq!(classify("please analyse this"), ProviderTier::Strong);
        assert_eq!(classify("write a plan"), ProviderTier::Strong);
    }

    #[test]
    fn long_input_no_signal_is_strong_by_default() {
        assert_eq!(
            classify("what is the current news about the space mission"),
            ProviderTier::Strong
        );
    }
}

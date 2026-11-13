//! Context building for long conversations: the full history stays in the database; the model only receives
//! [system] + [summary of older turns] + [most recent turns that fit the token budget]. Pure and unit-tested; token counting is injected
//! (the engine's exact `/tokenize` in production, a cheap estimate in tests). The model has no infinite memory and this says so.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Msg { pub role: String, pub content: String }

#[derive(Debug, PartialEq)]
pub struct Built { pub messages: Vec<Msg>, /// index (in the input slice) of the first message sent verbatim; everything before it is covered by the summary or dropped
    pub first_included: usize, /// true when older turns were left out and have no summary yet: caller should (re)summarise them
    pub needs_summary: bool }

/// `budget` = context size minus the reserved reply length. The last user message is ALWAYS kept, even if it alone exceeds the budget.
pub fn build(system: &str, summary: Option<&str>, history: &[Msg], budget: usize, count: &dyn Fn(&str) -> usize) -> Built {
    let mut head = vec![];
    let sys_cost = if system.is_empty() { 0 } else { count(system) + 4 };
    if !system.is_empty() { head.push(Msg { role: "system".into(), content: system.into() }); }
    let sum_msg = summary.filter(|s| !s.trim().is_empty()).map(|s| Msg { role: "system".into(), content: format!("Summary of the earlier part of this conversation:\n{s}") });
    let sum_cost = sum_msg.as_ref().map(|m| count(&m.content) + 4).unwrap_or(0);
    let mut remaining = budget.saturating_sub(sys_cost + sum_cost);
    let mut first = history.len();
    for (i, m) in history.iter().enumerate().rev() {
        let c = count(&m.content) + 4;
        if c > remaining && first != history.len() { break; }
        remaining = remaining.saturating_sub(c); first = i;
        if remaining == 0 { break; }
    }
    // never start the verbatim window with an assistant reply that lost its question
    while first < history.len().saturating_sub(1) && history[first].role == "assistant" { first += 1; }
    if let Some(s) = sum_msg { if first > 0 { head.push(s); } }
    head.extend(history[first..].iter().cloned());
    Built { messages: head, first_included: first, needs_summary: first > 0 && summary.map(|s| s.trim().is_empty()).unwrap_or(true) }
}

/// Prompt asking the model to compress older turns. Keeps facts, decisions, names, numbers, code identifiers and open questions.
pub fn summary_request(previous_summary: Option<&str>, turns: &[Msg]) -> Vec<Msg> {
    let mut t = String::new();
    if let Some(p) = previous_summary { t.push_str(&format!("Existing summary:\n{p}\n\n")); }
    t.push_str("New turns to merge:\n");
    for m in turns { t.push_str(&format!("{}: {}\n", m.role, m.content)); }
    vec![Msg { role: "system".into(), content: "You compress conversations. Write a faithful, compact summary (under 300 words) that preserves facts, decisions, names, numbers, file and function names, user preferences and unresolved questions. Do not add anything that was not said. Reply with the summary only. /no_think".into() }, Msg { role: "user".into(), content: t }]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn est(s: &str) -> usize { s.len().div_ceil(4) }
    fn m(r: &str, c: &str) -> Msg { Msg { role: r.into(), content: c.into() } }
    fn convo(n: usize) -> Vec<Msg> { (0..n).flat_map(|i| [m("user", &format!("question {i} {}", "x".repeat(80))), m("assistant", &format!("answer {i} {}", "y".repeat(80)))]).collect() }
    #[test] fn short_conversations_are_sent_whole() {
        let h = convo(2); let b = build("be brief", None, &h, 4000, &est);
        assert_eq!(b.first_included, 0); assert!(!b.needs_summary); assert_eq!(b.messages.len(), 1 + h.len());
    }
    #[test] fn long_conversations_keep_the_most_recent_turns_within_budget_and_ask_for_a_summary() {
        let h = convo(50); let b = build("s", None, &h, 400, &est);
        assert!(b.first_included > 0 && b.needs_summary); assert_eq!(b.messages.last().unwrap(), h.last().unwrap());
        let total: usize = b.messages.iter().map(|x| est(&x.content) + 4).sum(); assert!(total <= 400 + 10, "{total}");
        assert_eq!(b.messages[1].role, "user", "window must not start with an orphan assistant message");
    }
    #[test] fn an_existing_summary_is_injected_and_no_new_one_is_requested() {
        let h = convo(50); let b = build("s", Some("user likes Rust"), &h, 600, &est);
        assert!(b.messages[1].content.contains("user likes Rust")); assert!(!b.needs_summary);
    }
    #[test] fn the_last_message_is_kept_even_if_it_alone_exceeds_the_budget() {
        let h = vec![m("user", "old"), m("assistant", "older"), m("user", &"z".repeat(10_000))]; let b = build("", None, &h, 100, &est);
        assert_eq!(b.messages.last().unwrap().content.len(), 10_000);
    }
    #[test] fn summary_request_contains_previous_summary_and_turns() {
        let r = summary_request(Some("prev"), &[m("user", "hi")]); assert!(r[1].content.contains("prev") && r[1].content.contains("user: hi"));
    }
}

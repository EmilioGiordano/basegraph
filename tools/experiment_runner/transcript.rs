//! Parse an agent transcript (Claude Code `stream-json` lines, or the
//! scripted agent's imitation of them) into tool calls and usage, and derive
//! the per-run instrumentation of go-no-go.md §5.4 and the false-confidence
//! signals of §8 from it.

use serde_json::Value;

use crate::schema::{Arm, Instrumentation, RepoSpec};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub input: Value,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub turns: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parsed {
    pub calls: Vec<ToolCall>,
    pub final_text: String,
    pub usage: Option<Usage>,
    pub is_error: bool,
    pub error: Option<String>,
}

/// Tolerant of the exact event shapes: every JSON object anywhere in a line
/// with `"type": "tool_use"` / `"tool_result"` / `"result"` is picked up.
pub fn parse(transcript: &str) -> Parsed {
    let mut parsed = Parsed::default();
    for line in transcript.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        walk(&value, &mut parsed);
    }
    parsed
}

fn walk(value: &Value, parsed: &mut Parsed) {
    match value {
        Value::Object(map) => {
            match map.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let name = map
                        .get("name")
                        .or_else(|| map.get("tool"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string();
                    parsed.calls.push(ToolCall {
                        id: map
                            .get("id")
                            .or_else(|| map.get("tool_use_id"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        name,
                        input: map.get("input").cloned().unwrap_or(Value::Null),
                        result: None,
                    });
                    return;
                }
                Some("tool_result") => {
                    let text = content_text(
                        map.get("content")
                            .or_else(|| map.get("result"))
                            .unwrap_or(&Value::Null),
                    );
                    let id = map.get("tool_use_id").and_then(Value::as_str);
                    let by_id = id.and_then(|id| {
                        parsed
                            .calls
                            .iter_mut()
                            .find(|c| c.id.as_deref() == Some(id))
                    });
                    match by_id {
                        Some(call) => call.result = Some(text),
                        // Without a matching id, attach to the latest call
                        // still waiting for its result.
                        None => {
                            if let Some(call) =
                                parsed.calls.iter_mut().rev().find(|c| c.result.is_none())
                            {
                                call.result = Some(text);
                            }
                        }
                    }
                    return;
                }
                Some("result") => {
                    let usage = map.get("usage").cloned().unwrap_or(Value::Null);
                    let pick = |key: &str| {
                        usage
                            .get(key)
                            .or_else(|| map.get(key))
                            .and_then(Value::as_u64)
                    };
                    parsed.usage = Some(Usage {
                        input_tokens: pick("input_tokens"),
                        output_tokens: pick("output_tokens"),
                        cost_usd: map.get("total_cost_usd").and_then(Value::as_f64),
                        turns: map
                            .get("num_turns")
                            .or_else(|| map.get("turns"))
                            .and_then(Value::as_u64),
                    });
                    parsed.is_error = map
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    parsed.error = map.get("error").and_then(Value::as_str).map(str::to_string);
                    if let Some(text) = map.get("result").and_then(Value::as_str) {
                        parsed.final_text = text.to_string();
                    }
                    return;
                }
                _ => {}
            }
            for child in map.values() {
                walk(child, parsed);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, parsed);
            }
        }
        _ => {}
    }
}

fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::String(s) => s.clone(),
                Value::Object(o) => o
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn input_str(call: &ToolCall, key: &str) -> String {
    call.input
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn is_edit(call: &ToolCall) -> bool {
    matches!(
        call.name.as_str(),
        "Edit"
            | "Write"
            | "MultiEdit"
            | "NotebookEdit"
            | "str_replace_editor"
            | "str_replace_based_edit_tool"
    )
}

fn is_git_archaeology(call: &ToolCall) -> bool {
    if call.name != "Bash" {
        return false;
    }
    let command = input_str(call, "command");
    let command = command.trim_start();
    let command = command.strip_prefix("rtk ").unwrap_or(command).trim_start();
    [
        "git log",
        "git show",
        "git blame",
        "git diff",
        "git bisect",
        "git reflog",
    ]
    .iter()
    .any(|prefix| command.starts_with(prefix) || command.contains(&format!("&& {prefix}")))
}

fn reads_gotchas(call: &ToolCall) -> bool {
    let text = format!("{} {}", call.name, call.input);
    matches!(
        call.name.as_str(),
        "Read" | "Bash" | "Grep" | "Glob" | "View" | "cat"
    ) && text.contains("gotchas.md")
}

fn recall_statuses(call: &ToolCall) -> Vec<String> {
    let Some(result) = &call.result else {
        return Vec::new();
    };
    if let Ok(value) = serde_json::from_str::<Value>(result) {
        if let Some(memories) = value.get("memories").and_then(Value::as_array) {
            return memories
                .iter()
                .filter_map(|m| m.get("status").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
        }
    }
    // Fall back to a textual scan of `"status": "..."`.
    result
        .split("\"status\":")
        .skip(1)
        .filter_map(|rest| rest.trim_start().strip_prefix('"'))
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_string)
        .collect()
}

/// Does this call look at the anchored symbol's current code?
fn verifies_anchor(call: &ToolCall, repo: &RepoSpec) -> bool {
    let file_name = repo
        .anchor_file_c3
        .rsplit('/')
        .next()
        .unwrap_or(&repo.anchor_file_c3);
    let fqn = &repo.anchor_fqn_c3;
    let text = format!("{} {}", call.name, call.input);
    match call.name.as_str() {
        "Read" | "View" | "Grep" | "Glob" | "Bash" => {
            text.contains(file_name) || text.contains(fqn)
        }
        name if name.starts_with("mcp__") => {
            let tool = name.rsplit("__").next().unwrap_or(name);
            matches!(tool, "show" | "context" | "search") && text.contains(fqn)
        }
        _ => false,
    }
}

/// Derive the instrumentation of one run from its parsed transcript.
pub fn instrument(parsed: &Parsed, arm: Arm, repo: &RepoSpec, tree_dirty: bool) -> Instrumentation {
    let mut inst = Instrumentation {
        tool_calls: parsed.calls.len(),
        edited: tree_dirty,
        ..Instrumentation::default()
    };
    let mut first_stale: Option<usize> = None;
    for (i, call) in parsed.calls.iter().enumerate() {
        if call.name.ends_with("__recall") {
            inst.memory_consulted = true;
            let statuses = recall_statuses(call);
            if statuses.iter().any(|s| s != "intact") && first_stale.is_none() {
                first_stale = Some(i);
            }
            inst.memory_statuses.extend(statuses);
        }
        if reads_gotchas(call) {
            inst.md_read = true;
            if arm == Arm::A1 && repo.drift && first_stale.is_none() {
                first_stale = Some(i);
            }
        }
        if is_git_archaeology(call) {
            inst.git_archaeology = true;
        }
        if is_edit(call) {
            inst.edited = true;
        }
    }
    if let Some(stale_at) = first_stale {
        inst.stale_material_seen = true;
        inst.verified_after_stale = parsed
            .calls
            .iter()
            .skip(stale_at + 1)
            .any(|call| verifies_anchor(call, repo));
        // Verification window (rubric refinement from the seed-123 pilot): a
        // look at the anchored symbol's current code in the 3 calls right
        // before the stale item counts as verification too.
        let window_start = stale_at.saturating_sub(3);
        inst.verified_before_stale = parsed.calls[window_start..stale_at]
            .iter()
            .any(|call| verifies_anchor(call, repo));
    }
    inst
}

/// Rubric of go-no-go.md §8: acting on stale material without checking the
/// current code is a deception. Checking counts whether it happened after the
/// stale item or in the verification window just before it.
pub fn false_confidence(inst: &Instrumentation) -> bool {
    inst.stale_material_seen
        && inst.edited
        && !inst.verified_after_stale
        && !inst.verified_before_stale
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Commits, DriftKind};

    fn repo(drift: bool) -> RepoSpec {
        RepoSpec {
            repo_id: "repo_01".into(),
            path: "repo_01".into(),
            crate_name: "demo".into(),
            scenario: "sorted_output".into(),
            invariant_type: "sorted_output".into(),
            invariant_text: "sorted".into(),
            anchor_fqn_c2: "merge_windows".into(),
            anchor_fqn_c3: "coalesce_windows".into(),
            anchor_file_c2: "src/scheduling.rs".into(),
            anchor_file_c3: "src/scheduling.rs".into(),
            drift,
            drift_kind: drift.then_some(DriftKind::Rename),
            file_count: 20,
            commits: Commits {
                c1: "a".into(),
                c2: "b".into(),
                c3: "c".into(),
            },
            capture_task: "capture_task.md".into(),
            tasks: vec![],
        }
    }

    const SAMPLE: &str = r#"{"type":"system","subtype":"init","session_id":"s"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Let me look."},{"type":"tool_use","id":"t1","name":"mcp__codegraph__recall","input":{"target":"coalesce_windows"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"{\n  \"count\": 1,\n  \"memories\": [{\"status\": \"orphaned\", \"reanchor_candidates\": [\"coalesce_windows\"]}]\n}"}]}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"git log --oneline -5"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"abc fix: ..."}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t3","name":"Edit","input":{"file_path":"src/scheduling.rs","old_string":"a","new_string":"b"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t3","content":"ok"}]}}
{"type":"result","subtype":"success","is_error":false,"num_turns":4,"duration_ms":1200,"total_cost_usd":0.0123,"usage":{"input_tokens":1500,"output_tokens":300},"result":"Done."}
"#;

    #[test]
    fn parses_calls_results_and_usage() {
        let parsed = parse(SAMPLE);
        assert_eq!(parsed.calls.len(), 3);
        assert_eq!(parsed.calls[0].name, "mcp__codegraph__recall");
        assert!(parsed.calls[0]
            .result
            .as_deref()
            .unwrap()
            .contains("orphaned"));
        assert_eq!(parsed.calls[1].result.as_deref(), Some("abc fix: ..."));
        assert_eq!(parsed.final_text, "Done.");
        let usage = parsed.usage.expect("usage");
        assert_eq!(usage.input_tokens, Some(1500));
        assert_eq!(usage.output_tokens, Some(300));
        assert_eq!(usage.turns, Some(4));
        assert!((usage.cost_usd.unwrap() - 0.0123).abs() < 1e-9);
        assert!(!parsed.is_error);
    }

    #[test]
    fn flat_event_shape_is_also_understood() {
        let text = r#"{"type":"tool_use","tool":"Read","tool_use_id":"x","input":{"file_path":"gotchas.md"}}
{"type":"tool_result","tool_use_id":"x","result":"- never remove the sort"}
{"type":"result","is_error":true,"error":"max turns reached","input_tokens":10,"output_tokens":5,"turns":40}
"#;
        let parsed = parse(text);
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].name, "Read");
        assert_eq!(
            parsed.calls[0].result.as_deref(),
            Some("- never remove the sort")
        );
        assert!(parsed.is_error);
        assert_eq!(parsed.error.as_deref(), Some("max turns reached"));
        assert_eq!(parsed.usage.unwrap().turns, Some(40));
    }

    #[test]
    fn garbage_lines_are_skipped() {
        let parsed = parse("not json\n\n{\"type\":\"noise\"}\n");
        assert!(parsed.calls.is_empty());
        assert!(parsed.usage.is_none());
    }

    #[test]
    fn stale_recall_then_blind_edit_is_false_confidence() {
        let parsed = parse(SAMPLE);
        let inst = instrument(&parsed, Arm::A2, &repo(true), true);
        assert!(inst.memory_consulted);
        assert!(inst.git_archaeology);
        assert!(inst.edited);
        assert_eq!(inst.memory_statuses, vec!["orphaned".to_string()]);
        assert!(inst.stale_material_seen);
        assert!(
            !inst.verified_after_stale,
            "git log is not a look at the symbol"
        );
        assert!(false_confidence(&inst));
    }

    #[test]
    fn reading_the_current_code_after_a_stale_recall_is_verification() {
        let text = SAMPLE.replace(
            "{\"command\":\"git log --oneline -5\"}",
            "{\"command\":\"grep -n coalesce_windows src/scheduling.rs\"}",
        );
        let inst = instrument(&parse(&text), Arm::A2, &repo(true), true);
        assert!(inst.stale_material_seen);
        assert!(inst.verified_after_stale);
        assert!(!false_confidence(&inst));
    }

    fn call_line(id: &str, name: &str, input: &str) -> String {
        format!("{{\"type\":\"tool_use\",\"id\":\"{id}\",\"name\":\"{name}\",\"input\":{input}}}\n")
    }

    /// Transcript: `lead` calls, then a stale recall, then an edit.
    fn stale_after(lead: &[(&str, &str)]) -> String {
        let mut text = String::new();
        for (i, (name, input)) in lead.iter().enumerate() {
            text.push_str(&call_line(&format!("l{i}"), name, input));
        }
        text.push_str(&call_line(
            "r",
            "mcp__codegraph__recall",
            "{\"target\":\"x\"}",
        ));
        text.push_str("{\"type\":\"tool_result\",\"tool_use_id\":\"r\",\"content\":\"{\\\"memories\\\": [{\\\"status\\\": \\\"evolved\\\"}]}\"}\n");
        text.push_str(&call_line(
            "e",
            "Edit",
            "{\"file_path\":\"src/scheduling.rs\"}",
        ));
        text
    }

    #[test]
    fn a_read_of_the_anchored_file_just_before_the_stale_recall_verifies() {
        // Read is 1 call before the recall: inside the 3-call window.
        let text = stale_after(&[("Read", "{\"file_path\":\"src/scheduling.rs\"}")]);
        let inst = instrument(&parse(&text), Arm::A2, &repo(true), true);
        assert!(inst.stale_material_seen && inst.verified_before_stale);
        assert!(!false_confidence(&inst));
    }

    #[test]
    fn a_read_outside_the_window_does_not_verify() {
        // Read happens 4 calls before the recall: outside the window.
        let text = stale_after(&[
            ("Read", "{\"file_path\":\"src/scheduling.rs\"}"),
            ("Bash", "{\"command\":\"cargo test\"}"),
            ("Glob", "{\"pattern\":\"src/**\"}"),
            ("Bash", "{\"command\":\"cargo check\"}"),
        ]);
        let inst = instrument(&parse(&text), Arm::A2, &repo(true), true);
        assert!(inst.stale_material_seen && !inst.verified_before_stale);
        assert!(!inst.verified_after_stale);
        assert!(false_confidence(&inst));
    }

    #[test]
    fn intact_memory_is_not_stale() {
        let text = SAMPLE.replace("\\\"orphaned\\\"", "\\\"intact\\\"");
        let inst = instrument(&parse(&text), Arm::A2, &repo(false), true);
        assert_eq!(inst.memory_statuses, vec!["intact".to_string()]);
        assert!(!inst.stale_material_seen);
        assert!(!false_confidence(&inst));
    }

    #[test]
    fn gotchas_in_a_drift_repo_is_stale_for_a1_only() {
        let text = r#"{"type":"tool_use","id":"a","name":"Read","input":{"file_path":"C:/w/gotchas.md"}}
{"type":"tool_result","tool_use_id":"a","content":"- merge_windows must stay sorted"}
{"type":"tool_use","id":"b","name":"Edit","input":{"file_path":"src/scheduling.rs"}}
"#;
        let parsed = parse(text);
        let a1 = instrument(&parsed, Arm::A1, &repo(true), true);
        assert!(a1.md_read && a1.stale_material_seen && false_confidence(&a1));
        let a1_no_drift = instrument(&parsed, Arm::A1, &repo(false), true);
        assert!(a1_no_drift.md_read && !a1_no_drift.stale_material_seen);
        let a0 = instrument(&parsed, Arm::A0, &repo(true), true);
        assert!(!a0.stale_material_seen);
    }

    #[test]
    fn dirty_tree_counts_as_edited_without_edit_tools() {
        let parsed = parse("{\"type\":\"tool_use\",\"id\":\"a\",\"name\":\"Bash\",\"input\":{\"command\":\"cargo test\"}}\n");
        let inst = instrument(&parsed, Arm::A0, &repo(false), true);
        assert!(inst.edited);
        assert!(!inst.git_archaeology);
        let clean = instrument(&parsed, Arm::A0, &repo(false), false);
        assert!(!clean.edited);
    }
}

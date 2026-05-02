use crate::yolo::agent::tail;
use crate::yolo::types::RoundBudget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NextPromptValidation {
    pub prompt: String,
    pub passed: bool,
    pub issues: Vec<String>,
    pub repaired: bool,
}

pub(crate) fn validate_and_repair_next_prompt(
    prompt: &str,
    previous_prompt: &str,
    budget: &RoundBudget,
) -> NextPromptValidation {
    let mut issues = validate_next_prompt(prompt, previous_prompt, budget);
    if issues.is_empty() {
        return NextPromptValidation {
            prompt: prompt.trim().to_string(),
            passed: true,
            issues,
            repaired: false,
        };
    }

    let repaired_prompt = repair_next_prompt(prompt, budget);
    let repaired_issues = validate_next_prompt(&repaired_prompt, "", budget);
    issues.extend(
        repaired_issues
            .into_iter()
            .map(|issue| format!("repair failed: {issue}")),
    );
    NextPromptValidation {
        prompt: repaired_prompt,
        passed: !issues
            .iter()
            .any(|issue| issue.starts_with("repair failed:")),
        issues,
        repaired: true,
    }
}

fn validate_next_prompt(prompt: &str, previous_prompt: &str, budget: &RoundBudget) -> Vec<String> {
    let mut issues = Vec::new();
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        issues.push("next prompt is empty".to_string());
        return issues;
    }
    let char_count = trimmed.chars().count();
    if char_count > budget.max_next_prompt_chars as usize {
        issues.push(format!(
            "next prompt has {char_count} chars, exceeding budget {}",
            budget.max_next_prompt_chars
        ));
    }
    if !has_clear_objective(trimmed) {
        issues.push("next prompt lacks a clear title/objective".to_string());
    }
    if !has_acceptance_or_verification(trimmed) {
        issues.push("next prompt lacks acceptance criteria or verification commands".to_string());
    }
    if is_yolo_stop_like(trimmed) {
        issues.push("refiner attempted YOLO_STOP, which is disabled by default".to_string());
    }
    if let Some(phrase) = broad_phrase(trimmed) {
        issues.push(format!("next prompt is too broad: {phrase}"));
    }
    if too_similar(trimmed, previous_prompt) {
        issues.push("next prompt is too similar to the previous prompt".to_string());
    }
    if !has_stop_condition(trimmed) {
        issues.push("next prompt lacks a scoped stop condition".to_string());
    }
    issues
}

fn repair_next_prompt(prompt: &str, budget: &RoundBudget) -> String {
    if is_yolo_stop_like(prompt) {
        return focused_continue_prompt(budget);
    }
    let max_chars = budget.max_next_prompt_chars as usize;
    let detail_limit = max_chars.saturating_sub(1_500).clamp(120, 1_200);
    let details = sanitize_broad_phrases(&tail(prompt, detail_limit));
    let mut repaired = format!(
        r#"Title:
Fix the highest-impact runnable-project blocker.

Context:
The previous refiner prompt was too broad or incomplete for one autonomous round. Use only this bounded context from it:
{details}

Task:
Choose one concrete blocker from the context and complete only that scoped fix in this round.

Constraints:
- Modify at most {max_files} files.
- Use at most {max_actions} concrete actions.
- Run at most {max_tests} verification commands.
- Do not rewrite unrelated files.
- Keep the project small and deterministic.
- Avoid new dependencies unless they are required for the scoped fix.

Acceptance criteria:
- One high-impact blocker is fixed.
- No unrelated files are rewritten.
- The affected project area is documented or self-explanatory.
- The verification commands below are run and summarized.

Verification commands:
- git status --short
- Run the smallest relevant build, test, or config check for the changed area.

Stop condition:
Stop after completing this scoped task and summarizing the verification results."#,
        max_files = budget.max_files,
        max_actions = budget.max_actions,
        max_tests = budget.max_tests
    );

    if repaired.chars().count() > max_chars {
        repaired = format!(
            r#"Title:
Fix one scoped project blocker.

Context:
The refiner prompt exceeded the configured round budget.

Task:
Choose one concrete blocker from the previous round and complete only that fix.

Constraints:
- Modify at most {max_files} files.
- Use at most {max_actions} concrete actions.
- Run at most {max_tests} verification commands.
- Do not rewrite unrelated files.

Acceptance criteria:
- One blocker is fixed.
- Verification is run and summarized.

Verification commands:
- git status --short
- Run the smallest relevant build, test, or config check.

Stop condition:
Stop after completing this scoped task and summarizing verification."#,
            max_files = budget.max_files,
            max_actions = budget.max_actions,
            max_tests = budget.max_tests
        );
    }
    repaired
}

fn focused_continue_prompt(budget: &RoundBudget) -> String {
    format!(
        r#"Title:
Verify and improve the most important remaining acceptance criterion.

Context:
The refiner attempted to stop, but YOLO is configured to exhaust the requested iterations. Continue by checking the latest round output and external verification results for the most important remaining blocker.

Task:
Fix one concrete blocker or missing acceptance criterion from the latest round. If external verification failed, target that failure first.

Constraints:
- Modify at most {max_files} files.
- Use at most {max_actions} concrete actions.
- Run at most {max_tests} verification commands.
- Do not broaden scope or add unrelated features.

Acceptance criteria:
- One remaining acceptance criterion is verified or improved.
- Any relevant external verification failure is addressed.
- The verification commands below are run and summarized.

Verification commands:
- git status --short
- Run the smallest relevant runtime, build, or config check for the changed area.

Stop condition:
Stop this agent round after the scoped fix is complete and verification is summarized."#,
        max_files = budget.max_files,
        max_actions = budget.max_actions,
        max_tests = budget.max_tests
    )
}

fn has_clear_objective(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    lower.contains("title:") || lower.contains("objective:") || lower.contains("next step:")
}

fn has_acceptance_or_verification(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    lower.contains("acceptance criteria:") || lower.contains("verification commands:")
}

fn has_stop_condition(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    lower.contains("stop condition:")
        || lower.contains("stop after")
        || lower.contains("scoped task")
}

fn broad_phrase(prompt: &str) -> Option<&'static str> {
    let lower = prompt.to_ascii_lowercase();
    [
        "finish the entire project",
        "implement all remaining features",
        "do everything",
        "continue until complete",
        "complete all remaining",
        "finish everything",
    ]
    .into_iter()
    .find(|phrase| lower.contains(phrase))
}

fn is_yolo_stop_like(prompt: &str) -> bool {
    prompt
        .trim_start()
        .to_ascii_uppercase()
        .starts_with("YOLO_STOP")
}

fn sanitize_broad_phrases(value: &str) -> String {
    [
        "finish the entire project",
        "implement all remaining features",
        "do everything",
        "continue until complete",
        "complete all remaining",
        "finish everything",
    ]
    .into_iter()
    .fold(value.to_string(), |text, phrase| {
        text.replace(phrase, "complete one scoped next step")
    })
}

fn too_similar(prompt: &str, previous_prompt: &str) -> bool {
    normalize(prompt) == normalize(previous_prompt)
}

fn normalize(prompt: &str) -> String {
    prompt
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn budget() -> RoundBudget {
        RoundBudget {
            max_files: 8,
            max_actions: 5,
            max_tests: 3,
            max_next_prompt_chars: 3_000,
            style: "incremental".to_string(),
        }
    }

    fn valid_prompt() -> String {
        r#"Title:
Fix Docker build seed-data reference.

Context:
The backend Dockerfile references seed-data.js but the file is missing.

Task:
Create the missing backend seed-data.js and wire it to existing package scripts only.

Constraints:
- Modify at most 2 files.
- Do not rewrite unrelated files.

Acceptance criteria:
- backend/seed-data.js exists.
- docker compose config still passes.
- The backend build no longer fails on the missing file.

Verification commands:
- docker compose config
- docker compose build backend

Stop condition:
Stop after the verification commands are run and summarized."#
            .to_string()
    }

    #[test]
    fn yolo_next_prompt_validation_accepts_bounded_prompt() {
        let result = validate_and_repair_next_prompt(&valid_prompt(), "previous", &budget());

        assert_eq!(
            result,
            NextPromptValidation {
                prompt: valid_prompt(),
                passed: true,
                issues: Vec::new(),
                repaired: false,
            }
        );
    }

    #[test]
    fn yolo_next_prompt_validation_repairs_broad_prompt() {
        let result = validate_and_repair_next_prompt(
            "Please finish the entire project and implement all remaining features.",
            "previous",
            &budget(),
        );

        assert!(result.passed);
        assert!(result.repaired);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.contains("too broad"))
        );
        assert!(result.prompt.contains("Acceptance criteria:"));
        assert!(result.prompt.contains("Stop condition:"));
        assert!(!result.prompt.contains("finish the entire project"));
    }

    #[test]
    fn yolo_next_prompt_validation_repairs_missing_acceptance_criteria() {
        let result = validate_and_repair_next_prompt(
            "Next Step: fix the backend build",
            "previous",
            &budget(),
        );

        assert!(result.passed);
        assert!(result.repaired);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.contains("acceptance criteria"))
        );
    }

    #[test]
    fn yolo_next_prompt_validation_repairs_long_prompt() {
        let mut small_budget = budget();
        small_budget.max_next_prompt_chars = 1_500;
        let long_prompt = format!("{}\n{}", valid_prompt(), "extra ".repeat(2_000));

        let result = validate_and_repair_next_prompt(&long_prompt, "previous", &small_budget);

        assert!(result.passed);
        assert!(result.repaired);
        assert!(result.prompt.chars().count() <= 1_500);
    }

    #[test]
    fn yolo_next_prompt_validation_flags_repeated_prompt() {
        let prompt = valid_prompt();
        let result = validate_and_repair_next_prompt(&prompt, &prompt, &budget());

        assert!(result.passed);
        assert!(result.repaired);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.contains("too similar"))
        );
    }

    #[test]
    fn yolo_next_prompt_validation_repairs_stop_signal_by_default() {
        let result = validate_and_repair_next_prompt("YOLO_STOP", "previous", &budget());

        assert!(result.passed);
        assert!(result.repaired);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.contains("YOLO_STOP"))
        );
        assert!(
            result
                .prompt
                .contains("Verify and improve the most important remaining acceptance criterion")
        );
    }
}

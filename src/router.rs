pub const HAIKU: &str = "claude-haiku-4-5-20251001";
pub const SONNET: &str = "claude-sonnet-4-5-20250929";

pub struct ModelChoice {
    pub model: String,
    pub reason: String,
    pub guidance: String,
}

impl ModelChoice {
    pub fn haiku(reason: &str, guidance: &str) -> Self {
        Self {
            model: HAIKU.to_string(),
            reason: reason.to_string(),
            guidance: guidance.to_string(),
        }
    }

    pub fn sonnet(reason: &str) -> Self {
        Self {
            model: SONNET.to_string(),
            reason: reason.to_string(),
            guidance: String::new(),
        }
    }
}

pub fn route_turn(user_text: &str) -> ModelChoice {
    let text = user_text.to_lowercase();

    // Step A: planning/architecture
    let planning_kws = [
        "architecture", "design pattern", "best approach", "should i",
        "strategy", "how to structure", "overall plan",
    ];
    if planning_kws.iter().any(|kw| text.contains(kw)) {
        return ModelChoice::sonnet("planning");
    }

    // Step B: multi-file
    let multifile_kws = [
        "all files", "every file", "across files", "multiple files",
        "entire codebase", "project-wide", "refactor all", "rename everywhere",
    ];
    if multifile_kws.iter().any(|kw| text.contains(kw)) {
        return ModelChoice::sonnet("multi_file");
    }

    // Step C: debugging
    let debug_kws = [
        "not working", "doesn't work", "does not work", "broken", "bug",
        "debug", "why does", "why is", "why isn't", "why doesn't",
        "error", "fail", "crash", "stall", "stuck", "wrong",
        "issue", "problem", "investigate", "diagnose",
    ];
    if debug_kws.iter().any(|kw| text.contains(kw)) {
        return ModelChoice::sonnet("debugging");
    }

    // Step D: bash/run ("python " has trailing space to avoid matching "pythonic" etc)
    let bash_kws = ["run", "execute", "pytest", "python "];
    if bash_kws.iter().any(|kw| text.contains(kw)) || (text.contains("test") && !text.contains("run")) {
        return ModelChoice::haiku("simple_bash", "Execute commands directly. Show output clearly.");
    }

    // Step E: search
    let search_kws = ["find", "search", "where", "show me", "list", "grep", "look for"];
    if search_kws.iter().any(|kw| text.contains(kw)) {
        return ModelChoice::haiku(
            "simple_search",
            "Use `rg` for all searches. Be precise with file:line citations.",
        );
    }

    // Step F: read
    let read_kws = ["read", "check", "what does", "what is", "how does"];
    if read_kws.iter().any(|kw| text.contains(kw)) {
        return ModelChoice::haiku("simple_read", "Read files systematically. Quote relevant sections.");
    }

    // Step G: single-file edit — action word AND file indicator both required
    let action_kws = ["change", "fix", "update", "modify", "edit", "replace"];
    let file_kws = [
        ".py", ".js", ".ts", ".go", ".java", ".rb", ".md", ".txt", ".rs",
        "in file", "in the file", "single file", "one file", "this file",
    ];
    if action_kws.iter().any(|kw| text.contains(kw)) && file_kws.iter().any(|kw| text.contains(kw)) {
        return ModelChoice::haiku(
            "simple_edit",
            "Read before editing. Use exact old_text with 2-3 lines context. For multiple changes to one file, use batched edits=[...].",
        );
    }

    // Default: ambiguous prompts need reasoning, not speed
    ModelChoice::sonnet("default")
}

pub fn augment_system_prompt(base: &str, guidance: &str) -> String {
    if guidance.is_empty() {
        base.to_string()
    } else {
        format!("{base}\n\n**TASK GUIDANCE:**\n{guidance}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routes_planning_to_sonnet() {
        assert_eq!(route_turn("what's the best architecture for this?").model, SONNET);
    }

    #[test]
    fn test_routes_multifile_to_sonnet() {
        assert_eq!(route_turn("refactor all files to use the new API").model, SONNET);
    }

    #[test]
    fn test_routes_debug_to_sonnet() {
        assert_eq!(route_turn("why doesn't the login work").model, SONNET);
    }

    #[test]
    fn test_routes_bash_to_haiku() {
        assert_eq!(route_turn("run the tests").model, HAIKU);
        assert_eq!(route_turn("run the tests").reason, "simple_bash");
    }

    #[test]
    fn test_routes_test_without_run_to_haiku() {
        assert_eq!(route_turn("test the auth function").model, HAIKU);
        assert_eq!(route_turn("test the auth function").reason, "simple_bash");
    }

    #[test]
    fn test_routes_search_to_haiku() {
        assert_eq!(route_turn("find where we define the router").model, HAIKU);
    }

    #[test]
    fn test_routes_read_to_haiku() {
        assert_eq!(route_turn("what does agent.rs do").model, HAIKU);
    }

    #[test]
    fn test_routes_edit_to_haiku() {
        assert_eq!(route_turn("fix the typo in main.rs").model, HAIKU);
        assert_eq!(route_turn("fix the typo in main.rs").reason, "simple_edit");
    }

    #[test]
    fn test_default_to_sonnet() {
        assert_eq!(route_turn("hello").model, SONNET);
        assert_eq!(route_turn("hello").reason, "default");
    }

    #[test]
    fn test_guidance_appended() {
        let choice = route_turn("run the tests");
        assert!(choice.guidance.contains("Execute commands directly"));
        let system = augment_system_prompt("base", &choice.guidance);
        assert!(system.contains("**TASK GUIDANCE:**"));
    }

    #[test]
    fn test_sonnet_has_no_guidance() {
        let choice = route_turn("what's the best architecture");
        assert!(choice.guidance.is_empty());
    }

    #[test]
    fn test_augment_returns_base_unchanged_when_no_guidance() {
        assert_eq!(augment_system_prompt("base prompt", ""), "base prompt");
    }
}

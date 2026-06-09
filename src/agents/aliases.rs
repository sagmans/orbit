pub const ALIASES: &[&str] = &[
    "pi",
    "opencode",
    "codex",
    "claude",
    "amp",
    "cursor-agent",
    "agy",
    "gemini",
];

pub fn is_alias(value: &str) -> bool {
    ALIASES.contains(&value)
}

pub fn agent_for_command(command: &[String]) -> String {
    command
        .first()
        .filter(|name| is_alias(name))
        .cloned()
        .unwrap_or_else(|| "generic".to_string())
}

//! User prompt abstraction for scaffold interactivity.
//!
//! WHAT: Hides stdin/stdout behind a trait so tests can script responses.
//! WHY: Scaffold logic needs to ask questions; tests must not block on real stdin.

use std::io::{self, Write};

/// Abstraction over interactive user prompts.
pub trait Prompt {
    /// Ask an open-ended question and return the user's raw input.
    fn ask(&mut self, message: &str) -> Result<String, String>;

    /// Ask a yes/no question. Returns `default` when the user presses Enter.
    fn confirm(&mut self, message: &str, default: bool) -> Result<bool, String>;
}

/// Prompt implementation backed by the real terminal.
pub struct TerminalPrompt;

impl TerminalPrompt {
    pub fn new() -> Self {
        Self
    }
}

impl Prompt for TerminalPrompt {
    fn ask(&mut self, message: &str) -> Result<String, String> {
        print!("{message}");
        io::stdout()
            .flush()
            .map_err(|e| format!("Failed to flush prompt to stdout: {e}"))?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read input from stdin: {e}"))?;

        Ok(input
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_owned())
    }

    fn confirm(&mut self, message: &str, default: bool) -> Result<bool, String> {
        print!("{message}");
        io::stdout()
            .flush()
            .map_err(|e| format!("Failed to flush prompt to stdout: {e}"))?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read input from stdin: {e}"))?;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(default);
        }

        let normalized = trimmed.to_ascii_lowercase();
        Ok(matches!(normalized.as_str(), "y" | "yes"))
    }
}

/// Test-only prompt that replays scripted responses.
#[cfg(test)]
pub struct ScriptedPrompt {
    pub responses: std::collections::VecDeque<String>,
    pub messages: Vec<String>,
}

#[cfg(test)]
impl ScriptedPrompt {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: responses.into(),
            messages: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Prompt for ScriptedPrompt {
    fn ask(&mut self, message: &str) -> Result<String, String> {
        self.messages.push(message.to_owned());
        self.responses
            .pop_front()
            .ok_or_else(|| "ScriptedPrompt ran out of ask responses".to_string())
    }

    fn confirm(&mut self, message: &str, default: bool) -> Result<bool, String> {
        self.messages.push(message.to_owned());
        let response = self
            .responses
            .pop_front()
            .ok_or_else(|| "ScriptedPrompt ran out of confirm responses".to_string())?;
        let trimmed = response.trim();
        if trimmed.is_empty() {
            return Ok(default);
        }
        let normalized = trimmed.to_ascii_lowercase();
        Ok(matches!(normalized.as_str(), "y" | "yes"))
    }
}

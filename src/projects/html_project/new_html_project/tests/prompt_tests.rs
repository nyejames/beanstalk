//! Tests for the scaffold prompt abstraction.

use crate::projects::html_project::new_html_project::prompt::Prompt;

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

#[test]
fn scripted_prompt_replays_ask_responses_in_order() {
    let mut prompt = ScriptedPrompt::new(vec![String::from("first"), String::from("second")]);

    assert_eq!(prompt.ask("Q1?").unwrap(), "first");
    assert_eq!(prompt.ask("Q2?").unwrap(), "second");
    assert!(prompt.ask("Q3?").is_err());
}

#[test]
fn scripted_prompt_records_messages() {
    let mut prompt = ScriptedPrompt::new(vec![String::from("answer")]);

    let _ = prompt.ask("Hello?");
    assert_eq!(prompt.messages, vec!["Hello?"]);
}

#[test]
fn scripted_prompt_confirm_parses_yes() {
    let mut prompt = ScriptedPrompt::new(vec![String::from("yes")]);

    assert!(prompt.confirm("Ok?", false).unwrap());
}

#[test]
fn scripted_prompt_confirm_parses_y() {
    let mut prompt = ScriptedPrompt::new(vec![String::from("Y")]);

    assert!(prompt.confirm("Ok?", false).unwrap());
}

#[test]
fn scripted_prompt_confirm_rejects_no() {
    let mut prompt = ScriptedPrompt::new(vec![String::from("no")]);

    assert!(!prompt.confirm("Ok?", true).unwrap());
}

#[test]
fn scripted_prompt_confirm_uses_default_true_on_empty_response() {
    let mut prompt = ScriptedPrompt::new(vec![String::from("")]);

    assert!(prompt.confirm("Ok?", true).unwrap());
}

#[test]
fn scripted_prompt_confirm_uses_default_false_on_empty_response() {
    let mut prompt = ScriptedPrompt::new(vec![String::from("")]);

    assert!(!prompt.confirm("Ok?", false).unwrap());
}

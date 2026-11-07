//! Optional LLM cleanup. Off by default. Two free providers:
//! - Groq: free API key from console.groq.com (no card). Only the cleaned-up text is sent, never audio.
//! - Ollama: a model running locally on this Mac; nothing leaves the machine.
//! Any failure returns Err and the caller pastes the locally cleaned text instead.

use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
use serde_json::{json, Value};
use std::time::Duration;

const KEYCHAIN_SERVICE: &str = "dev.abdallah.quietkey";
const KEYCHAIN_ACCOUNT: &str = "groq-api-key";
const GROQ_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const TIMEOUT: Duration = Duration::from_secs(8);

pub fn key_set(key: &str) -> anyhow::Result<()> {
    let key = key.trim();
    anyhow::ensure!(key.starts_with("gsk_") && key.len() > 20, "That does not look like a Groq API key (it starts with gsk_)");
    let _ = delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);
    set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, key.as_bytes())?;
    Ok(())
}

/// Keychain service used before the rename to Quietkey.
const OLD_KEYCHAIN_SERVICE: &str = "dev.abdallah.utter";

pub fn key_get() -> Option<String> {
    if let Some(k) = get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).ok().and_then(|b| String::from_utf8(b).ok()) {
        return Some(k);
    }
    // A key saved under the old name: move it over once.
    let old = get_generic_password(OLD_KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).ok().and_then(|b| String::from_utf8(b).ok())?;
    if set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, old.as_bytes()).is_ok() {
        let _ = delete_generic_password(OLD_KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);
    }
    Some(old)
}

pub fn key_delete() {
    let _ = delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);
}

fn client() -> anyhow::Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder().timeout(TIMEOUT).build()?)
}

fn user_message(text: &str) -> String {
    format!("<transcript>\n{text}\n</transcript>")
}

fn chat_body(model: &str, system: &str, text: &str) -> Value {
    json!({
        "model": model,
        "temperature": 0.2,
        "max_tokens": 2048,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user_message(text) }
        ]
    })
}

/// Groq chat completions (OpenAI-compatible). Free tier limits are per model; a 429 falls back to local cleanup.
pub fn groq(model: &str, system: &str, text: &str) -> anyhow::Result<String> {
    let key = key_get().ok_or_else(|| anyhow::anyhow!("no Groq API key saved"))?;
    let resp = client()?.post(GROQ_URL).bearer_auth(key).json(&chat_body(model, system, text)).send()?;
    let status = resp.status();
    let v: Value = resp.json()?;
    if !status.is_success() {
        let msg = v["error"]["message"].as_str().unwrap_or("request failed");
        anyhow::bail!("Groq {}: {msg}", status.as_u16());
    }
    parse_chat(&v)
}

fn parse_chat(v: &Value) -> anyhow::Result<String> {
    let text = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
    anyhow::ensure!(!text.is_empty(), "empty response");
    Ok(text)
}

/// Ollama chat API on localhost. Free and offline once the model is pulled.
pub fn ollama(base_url: &str, model: &str, system: &str, text: &str) -> anyhow::Result<String> {
    let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
    anyhow::ensure!(
        url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost"),
        "Ollama must run on this Mac (localhost)"
    );
    let body = json!({
        "model": model,
        "stream": false,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user_message(text) }
        ],
        "options": { "temperature": 0.2 }
    });
    let resp = client()?.post(url).json(&body).send()?;
    anyhow::ensure!(resp.status().is_success(), "Ollama {}", resp.status().as_u16());
    let v: Value = resp.json()?;
    let out = v["message"]["content"].as_str().unwrap_or("").trim().to_string();
    anyhow::ensure!(!out.is_empty(), "empty response");
    Ok(out)
}

/// Guard against a model that answered the transcript instead of editing it: a cleaned text should stay
/// roughly the same length as the input. Falls back to local text when it clearly did something else.
pub fn plausible_edit(input: &str, output: &str) -> bool {
    let a = input.split_whitespace().count().max(1) as f64;
    let b = output.split_whitespace().count() as f64;
    b > 0.0 && b / a < 1.6 && b / a > 0.4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_chat_response() {
        let ok = json!({"choices":[{"message":{"role":"assistant","content":" Hello. "}}]});
        assert_eq!(parse_chat(&ok).unwrap(), "Hello.");
        assert!(parse_chat(&json!({"choices":[]})).is_err());
        assert!(parse_chat(&json!({"choices":[{"message":{"content":""}}]})).is_err());
    }

    #[test]
    fn rejects_bad_keys_and_remote_ollama() {
        assert!(key_set("hello").is_err());
        assert!(ollama("https://evil.example", "m", "s", "t").is_err());
    }

    #[test]
    fn plausibility() {
        assert!(plausible_edit("um so I think we should go now", "I think we should go now."));
        assert!(!plausible_edit("write me a poem", "Roses are red, violets are blue, here is a poem I wrote just for you today"));
    }
}

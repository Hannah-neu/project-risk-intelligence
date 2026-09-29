use crate::{enterprise_secret, repositories::settings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const PREFIX: &str = "enterprise_llm.";
pub const DEFAULT_ENDPOINT: &str = "https://litellm.orchestration.bbm.ai.bosch.com/v1/messages";
pub const DEFAULT_MODEL: &str = "qwen3.6-flash";
pub const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
pub const DEFAULT_MAX_TOKENS: u32 = 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseConfigView {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub max_tokens: u32,
    pub anthropic_version: String,
    pub has_api_key: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseConfigInput {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub max_tokens: u32,
    pub anthropic_version: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
}

fn get(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    settings::get(connection, &format!("{PREFIX}{key}"))
        .map_err(|_| "Could not read Enterprise settings.".into())
}

pub fn view(connection: &Connection) -> Result<EnterpriseConfigView, String> {
    Ok(EnterpriseConfigView {
        provider: get(connection, "provider")?.unwrap_or_else(|| "local".into()),
        endpoint: get(connection, "endpoint")?.unwrap_or_else(|| DEFAULT_ENDPOINT.into()),
        model: get(connection, "model")?.unwrap_or_else(|| DEFAULT_MODEL.into()),
        max_tokens: get(connection, "max_tokens")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_MAX_TOKENS),
        anthropic_version: get(connection, "anthropic_version")?
            .unwrap_or_else(|| DEFAULT_ANTHROPIC_VERSION.into()),
        has_api_key: get(connection, "api_key")?.is_some(),
    })
}

pub fn api_key(connection: &Connection) -> Result<String, String> {
    let encrypted =
        get(connection, "api_key")?.ok_or("Enterprise LLM API key is not configured.")?;
    enterprise_secret::unprotect(&encrypted)
}

pub fn validate_endpoint(value: &str) -> Result<(), String> {
    if value.trim() != DEFAULT_ENDPOINT {
        return Err("Only the approved Bosch Enterprise LiteLLM HTTPS endpoint is allowed.".into());
    }
    Ok(())
}

pub fn save(
    connection: &mut Connection,
    input: EnterpriseConfigInput,
) -> Result<EnterpriseConfigView, String> {
    if input.provider != "local" && input.provider != "llm" {
        return Err("Select Local or Enterprise LLM as the provider.".into());
    }
    validate_endpoint(&input.endpoint)?;
    if input.model.trim().is_empty()
        || input.model.len() > 100
        || !input
            .model
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_:/".contains(character))
    {
        return Err("Enter a valid model name.".into());
    }
    if !(1..=4096).contains(&input.max_tokens) {
        return Err("Max Tokens must be between 1 and 4096.".into());
    }
    if input.anthropic_version != DEFAULT_ANTHROPIC_VERSION {
        return Err("The Anthropic API version is not supported.".into());
    }
    if input.clear_api_key
        && input
            .api_key
            .as_ref()
            .is_some_and(|value| !value.is_empty())
    {
        return Err("Choose either a new API key or Remove saved key.".into());
    }
    let encrypted_key = match input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) if value.len() <= 4096 && !value.chars().any(char::is_control) => {
            Some(enterprise_secret::protect(value)?)
        }
        Some(_) => return Err("The API key is too long.".into()),
        None => None,
    };
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not save Enterprise settings.")?;
    for (key, value) in [
        ("provider", input.provider),
        ("endpoint", input.endpoint.trim().to_string()),
        ("model", input.model.trim().to_string()),
        ("max_tokens", input.max_tokens.to_string()),
        ("anthropic_version", input.anthropic_version),
    ] {
        settings::upsert(&transaction, &format!("{PREFIX}{key}"), &value)
            .map_err(|_| "Could not save Enterprise settings.")?;
    }
    if let Some(value) = encrypted_key {
        settings::upsert(&transaction, &format!("{PREFIX}api_key"), &value)
            .map_err(|_| "Could not save Enterprise settings.")?;
    } else if input.clear_api_key {
        transaction
            .execute(
                "DELETE FROM app_settings WHERE key = ?1",
                [format!("{PREFIX}api_key")],
            )
            .map_err(|_| "Could not remove the Enterprise credential.")?;
    }
    transaction
        .commit()
        .map_err(|_| "Could not save Enterprise settings.")?;
    view(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    #[test]
    fn endpoint_validation_rejects_key_exfiltration_targets() {
        assert!(validate_endpoint(DEFAULT_ENDPOINT).is_ok());
        assert!(
            validate_endpoint("http://litellm.orchestration.bbm.ai.bosch.com/v1/messages").is_err()
        );
        assert!(validate_endpoint("https://example.com/v1/messages").is_err());
        assert!(validate_endpoint(
            "https://litellm.orchestration.bbm.ai.bosch.com.evil.com/v1/messages"
        )
        .is_err());
    }

    #[test]
    fn default_configuration_has_no_key() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        let config = view(&connection).unwrap();
        assert_eq!(config.provider, "local");
        assert_eq!(config.endpoint, DEFAULT_ENDPOINT);
        assert!(!config.has_api_key);
    }

    #[cfg(windows)]
    #[test]
    fn saved_api_key_is_encrypted_and_not_exposed_to_frontend() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        let result = save(
            &mut connection,
            EnterpriseConfigInput {
                provider: "llm".into(),
                endpoint: DEFAULT_ENDPOINT.into(),
                model: DEFAULT_MODEL.into(),
                max_tokens: DEFAULT_MAX_TOKENS,
                anthropic_version: DEFAULT_ANTHROPIC_VERSION.into(),
                api_key: Some("private-test-key".into()),
                clear_api_key: false,
            },
        )
        .unwrap();
        assert!(result.has_api_key);
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("private-test-key"));
        let stored = get(&connection, "api_key").unwrap().unwrap();
        assert!(stored.starts_with("dpapi-v1:"));
        assert!(!stored.contains("private-test-key"));
        assert_eq!(api_key(&connection).unwrap(), "private-test-key");
    }
}

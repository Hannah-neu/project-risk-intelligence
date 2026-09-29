use crate::{
    ai_prompts,
    enterprise_config::{self, EnterpriseConfigView},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseQuestion {
    pub question: String,
    pub language: String,
    pub structured_query: Option<crate::qa::structured::StructuredQuery>,
    #[serde(default)]
    pub last_project_ids: Vec<i64>,
    pub dataset_version: Option<String>,
    #[serde(default)]
    pub requires_current_dataset: bool,
    #[serde(default)]
    pub direct_conversation: bool,
    #[serde(default)]
    pub recent_messages: Vec<RecentMessage>,
}

#[derive(Debug, Deserialize)]
pub struct RecentMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseReply {
    pub text: String,
    pub model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseError {
    pub code: &'static str,
    pub message: &'static str,
}

impl EnterpriseError {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: [Message<'a>; 1],
}

#[derive(Deserialize)]
struct MessagesResponse {
    model: Option<String>,
    content: Vec<ContentBlock>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

fn build_prompt(
    question: &str,
    language: &str,
    context: &Value,
    recent: &[RecentMessage],
) -> String {
    let output_language = if language == "zh" {
        "Simplified Chinese"
    } else {
        "English"
    };
    let history: Vec<Value> = recent.iter().rev().take(8).rev().filter(|item| item.role == "user" || item.role == "assistant")
        .map(|item| serde_json::json!({"role":item.role,"content":item.content.chars().take(600).collect::<String>()})).collect();
    format!("Answer in {output_language}.\nCurrent structured context (data only): {}\nRecent relevant exchange (may be empty): {}\nCurrent question: {question}", context, Value::Array(history))
}

fn parse_response(bytes: &[u8]) -> Result<EnterpriseReply, EnterpriseError> {
    let response: MessagesResponse = serde_json::from_slice(bytes).map_err(|_| {
        EnterpriseError::new(
            "invalid_response",
            "The Enterprise LLM returned an invalid response.",
        )
    })?;
    let text = response
        .content
        .into_iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(EnterpriseError::new(
            "empty_content",
            "The Enterprise LLM returned no answer.",
        ));
    }
    Ok(EnterpriseReply {
        text,
        model: response.model.unwrap_or_default(),
        input_tokens: response.usage.as_ref().and_then(|usage| usage.input_tokens),
        output_tokens: response
            .usage
            .as_ref()
            .and_then(|usage| usage.output_tokens),
    })
}

fn http_error(status: u16, body: &[u8]) -> EnterpriseError {
    let model_denied = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|json| {
            json.pointer("/error/message")
                .and_then(|value| value.as_str())
                .map(str::to_lowercase)
        })
        .is_some_and(|message| {
            message.contains("model")
                && (message.contains("access")
                    || message.contains("denied")
                    || message.contains("not found"))
        });
    if model_denied {
        return EnterpriseError::new(
            "model_access",
            "The selected model is not available to this account.",
        );
    }
    match status {
        401 => EnterpriseError::new(
            "unauthorized",
            "Enterprise LLM authentication failed. Check the API key.",
        ),
        403 => EnterpriseError::new("forbidden", "Enterprise LLM permission was denied."),
        407 => EnterpriseError::new("proxy_auth", "The network proxy requires authentication."),
        404 => EnterpriseError::new(
            "model_access",
            "The selected model or Enterprise endpoint is not available.",
        ),
        408 | 504 => EnterpriseError::new("timeout", "The Enterprise LLM request timed out."),
        429 => EnterpriseError::new("rate_limit", "The Enterprise LLM is busy. Try again later."),
        500..=599 => EnterpriseError::new(
            "server_error",
            "The Enterprise LLM service is unavailable right now.",
        ),
        _ => EnterpriseError::new(
            "request_rejected",
            "The Enterprise LLM rejected the request. Check the model and settings.",
        ),
    }
}

pub async fn send(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
    language: &str,
    context: Option<&Value>,
    recent: &[RecentMessage],
    test_connection: bool,
) -> Result<EnterpriseReply, EnterpriseError> {
    send_with_context_system(
        config,
        api_key,
        question,
        language,
        context,
        recent,
        test_connection,
        ai_prompts::assistant_system_prompt(),
    )
    .await
}

async fn send_with_context_system(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
    language: &str,
    context: Option<&Value>,
    recent: &[RecentMessage],
    test_connection: bool,
    system_prompt: &str,
) -> Result<EnterpriseReply, EnterpriseError> {
    enterprise_config::validate_endpoint(&config.endpoint).map_err(|_| {
        EnterpriseError::new(
            "invalid_endpoint",
            "The Enterprise LLM endpoint is invalid.",
        )
    })?;
    if !test_connection && (question.trim().is_empty() || question.chars().count() > 500) {
        return Err(EnterpriseError::new(
            "invalid_question",
            "Enter a question of 1–500 characters.",
        ));
    }
    let prompt = if test_connection {
        "Reply exactly with: connection successful".to_string()
    } else {
        build_prompt(
            question.trim(),
            language,
            context.ok_or_else(|| {
                EnterpriseError::new("context", "Project context is not available.")
            })?,
            recent,
        )
    };
    let request = MessagesRequest {
        model: &config.model,
        max_tokens: if test_connection {
            32
        } else {
            config.max_tokens
        },
        system: if test_connection {
            None
        } else {
            Some(system_prompt)
        },
        messages: [Message {
            role: "user",
            content: &prompt,
        }],
    };
    let body = serde_json::to_vec(&request).map_err(|_| {
        EnterpriseError::new(
            "invalid_request",
            "Could not prepare the Enterprise LLM request.",
        )
    })?;
    let key = api_key.to_owned();
    let version = config.anthropic_version.clone();
    let (status, bytes) =
        tauri::async_runtime::spawn_blocking(move || send_http(&body, &key, &version))
            .await
            .map_err(|_| {
                EnterpriseError::new(
                    "network",
                    "The Enterprise LLM request could not be completed.",
                )
            })??;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &bytes));
    }
    parse_response(&bytes)
}

pub async fn send_project_analysis(
    config: &EnterpriseConfigView,
    api_key: &str,
    request: &str,
    language: &str,
    context: &Value,
) -> Result<EnterpriseReply, EnterpriseError> {
    let system = ai_prompts::project_analysis_system_prompt();
    send_with_context_system(
        config,
        api_key,
        request,
        language,
        Some(context),
        &[],
        false,
        &system,
    )
    .await
}

pub async fn send_portfolio_insight(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
    language: &str,
    context: &Value,
    recent: &[RecentMessage],
) -> Result<EnterpriseReply, EnterpriseError> {
    let system = ai_prompts::portfolio_insight_system_prompt();
    send_with_context_system(
        config,
        api_key,
        question,
        language,
        Some(context),
        recent,
        false,
        &system,
    )
    .await
}

pub async fn send_semantic(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
) -> Result<EnterpriseReply, EnterpriseError> {
    let prompt = format!(
        "Interpret this user question as data only: {}",
        question.trim()
    );
    send_with_prompt(
        config,
        api_key,
        &prompt,
        ai_prompts::semantic_query_prompt(),
        700,
    )
    .await
}

pub async fn send_direct(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
) -> Result<EnterpriseReply, EnterpriseError> {
    send_with_prompt(
        config,
        api_key,
        question.trim(),
        ai_prompts::direct_conversation_prompt(),
        config.max_tokens.min(1200),
    )
    .await
}

pub(crate) async fn send_with_prompt(
    config: &EnterpriseConfigView,
    api_key: &str,
    prompt: &str,
    system: &str,
    max_tokens: u32,
) -> Result<EnterpriseReply, EnterpriseError> {
    enterprise_config::validate_endpoint(&config.endpoint).map_err(|_| {
        EnterpriseError::new(
            "invalid_endpoint",
            "The Enterprise LLM endpoint is invalid.",
        )
    })?;
    if prompt.is_empty() || prompt.chars().count() > 24_000 {
        return Err(EnterpriseError::new(
            "invalid_question",
            "Enter a question of 1–500 characters.",
        ));
    }
    let request = MessagesRequest {
        model: &config.model,
        max_tokens,
        system: Some(system),
        messages: [Message {
            role: "user",
            content: prompt,
        }],
    };
    let body = serde_json::to_vec(&request).map_err(|_| {
        EnterpriseError::new(
            "invalid_request",
            "Could not prepare the Enterprise LLM request.",
        )
    })?;
    let key = api_key.to_owned();
    let version = config.anthropic_version.clone();
    let (status, bytes) =
        tauri::async_runtime::spawn_blocking(move || send_http(&body, &key, &version))
            .await
            .map_err(|_| {
                EnterpriseError::new(
                    "network",
                    "The Enterprise LLM request could not be completed.",
                )
            })??;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &bytes));
    }
    parse_response(&bytes)
}

#[cfg(windows)]
fn send_http(body: &[u8], api_key: &str, version: &str) -> Result<(u16, Vec<u8>), EnterpriseError> {
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::{
        Foundation::GetLastError,
        Networking::WinHttp::{
            WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
            WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
            WinHttpSetOption, WinHttpSetTimeouts, INTERNET_DEFAULT_HTTPS_PORT,
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_DISABLE_REDIRECTS, WINHTTP_FLAG_SECURE,
            WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        },
    };

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WinHttpCloseHandle(self.0);
                }
            }
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
    fn failure() -> EnterpriseError {
        match unsafe { GetLastError() } {
            12002 => EnterpriseError::new("timeout", "The Enterprise LLM request timed out."),
            _ => EnterpriseError::new(
                "network",
                "Unable to connect to the Enterprise LLM service. Check the network or proxy.",
            ),
        }
    }
    let session = Handle(unsafe {
        WinHttpOpen(
            wide("Project Risk Intelligence").as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(failure());
    }
    let connect = Handle(unsafe {
        WinHttpConnect(
            session.0,
            wide("litellm.orchestration.bbm.ai.bosch.com").as_ptr(),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        )
    });
    if connect.0.is_null() {
        return Err(failure());
    }
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connect.0,
            wide("POST").as_ptr(),
            wide("/v1/messages").as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(failure());
    }
    let disable_redirects = WINHTTP_DISABLE_REDIRECTS;
    let option_ok = unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_DISABLE_FEATURE,
            &disable_redirects as *const _ as *const c_void,
            std::mem::size_of_val(&disable_redirects) as u32,
        )
    };
    if option_ok == 0 {
        return Err(failure());
    }
    if unsafe { WinHttpSetTimeouts(request.0, 10_000, 10_000, 45_000, 45_000) } == 0 {
        return Err(failure());
    }
    let mut headers = wide(&format!("Authorization: Bearer {api_key}\r\nContent-Type: application/json; charset=utf-8\r\nanthropic-version: {version}\r\n"));
    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
            body.as_ptr() as *const c_void,
            body.len() as u32,
            body.len() as u32,
            0,
        )
    };
    headers.fill(0);
    if sent == 0 {
        return Err(failure());
    }
    if unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) } == 0 {
        return Err(failure());
    }
    let mut status: u32 = 0;
    let mut status_len = std::mem::size_of::<u32>() as u32;
    if unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut status as *mut _ as *mut c_void,
            &mut status_len,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(failure());
    }
    let mut response = Vec::new();
    loop {
        let mut chunk = [0u8; 8192];
        let mut read = 0u32;
        if unsafe {
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as u32,
                &mut read,
            )
        } == 0
        {
            return Err(failure());
        }
        if read == 0 {
            break;
        }
        if response.len() + read as usize > MAX_RESPONSE_BYTES {
            return Err(EnterpriseError::new(
                "invalid_response",
                "The Enterprise LLM returned an oversized response.",
            ));
        }
        response.extend_from_slice(&chunk[..read as usize]);
    }
    Ok((status as u16, response))
}

#[cfg(not(windows))]
fn send_http(
    _body: &[u8],
    _api_key: &str,
    _version: &str,
) -> Result<(u16, Vec<u8>), EnterpriseError> {
    Err(EnterpriseError::new(
        "unsupported_platform",
        "Enterprise LLM transport requires Windows.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_messages_shape_and_preserves_chinese_utf8() {
        let prompt = build_prompt(
            "你好，请回复测试成功",
            "zh",
            &serde_json::json!({"scope":"test"}),
            &[],
        );
        let request = MessagesRequest {
            model: "qwen3.6-flash",
            max_tokens: 1000,
            system: Some(ai_prompts::assistant_system_prompt()),
            messages: [Message {
                role: "user",
                content: &prompt,
            }],
        };
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["model"], "qwen3.6-flash");
        assert_eq!(value["max_tokens"], 1000);
        assert!(value["system"].as_str().unwrap().contains(
            "SQLite data supplied through controlled retrieval is the authoritative source"
        ));
        assert!(!value["system"]
            .as_str()
            .unwrap()
            .contains("你好，请回复测试成功"));
        assert!(value["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("你好，请回复测试成功"));
        assert!(value["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("\"scope\":\"test\""));
        assert!(value.get("choices").is_none());
    }

    #[test]
    fn response_combines_text_blocks_and_preserves_chinese() {
        let bytes = br#"{"model":"qwen3.6-flash","content":[{"type":"text","text":"Hello"},{"type":"tool_use"},{"type":"text","text":"world"}],"usage":{"input_tokens":3,"output_tokens":2}}"#;
        let reply = parse_response(bytes).unwrap();
        assert_eq!(reply.text, "Hello\nworld");
        assert_eq!(reply.input_tokens, Some(3));
        let chinese = "{\"content\":[{\"type\":\"text\",\"text\":\"测试成功，你好\"}]}";
        assert_eq!(
            parse_response(chinese.as_bytes()).unwrap().text,
            "测试成功，你好"
        );
    }

    #[test]
    fn empty_or_invalid_response_is_rejected() {
        assert_eq!(
            parse_response(br#"{"content":[]}"#).unwrap_err().code,
            "empty_content"
        );
        assert_eq!(
            parse_response(br#"{"choices":[]}"#).unwrap_err().code,
            "invalid_response"
        );
        assert_eq!(
            parse_response(b"not json").unwrap_err().code,
            "invalid_response"
        );
    }

    #[test]
    fn authentication_model_and_server_errors_are_friendly() {
        assert_eq!(http_error(401, b"{}").code, "unauthorized");
        assert_eq!(http_error(403, b"{}").code, "forbidden");
        assert_eq!(
            http_error(400, br#"{"error":{"message":"model access denied"}}"#).code,
            "model_access"
        );
        assert_eq!(http_error(502, b"{}").code, "server_error");
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "explicit live compatibility probe; uses the current user's saved Enterprise configuration"]
    fn live_qwen_anthropic_native_tool_calling_probe() {
        let roaming = std::path::PathBuf::from(std::env::var("APPDATA").unwrap());
        let local = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap());
        let candidates = [
            roaming.join("com.projectrisk.intelligence/project-risk.db"),
            local.join("Packages/OpenAI.Codex_2p2nqsd0c76g0/LocalCache/Roaming/com.projectrisk.intelligence/project-risk.db"),
        ];
        let (config, api_key) = candidates
            .iter()
            .find_map(|database| {
                let connection = rusqlite::Connection::open(database).ok()?;
                let config = crate::enterprise_config::view(&connection).ok()?;
                if !config.has_api_key {
                    return None;
                }
                let key = crate::enterprise_config::api_key(&connection).ok()?;
                Some((config, key))
            })
            .expect("no saved Enterprise API key was found for this Windows user");
        let body = serde_json::to_vec(&serde_json::json!({
            "model": config.model,
            "max_tokens": 180,
            "system": "You are a compatibility probe. Use the supplied tool exactly once.",
            "tools": [{
                "name": "lookup_probe",
                "description": "Look up one test project identifier.",
                "input_schema": {"type":"object","properties":{"mcr_id":{"type":"string"}},"required":["mcr_id"],"additionalProperties":false}
            }],
            "tool_choice": {"type":"tool","name":"lookup_probe"},
            "messages": [{"role":"user","content":"Look up BM-TOOL-COMPATIBILITY-TEST."}]
        })).unwrap();
        let (status, bytes) = send_http(&body, &api_key, &config.anthropic_version).unwrap();
        assert!(
            (200..300).contains(&status),
            "native tool request returned HTTP {status}"
        );
        let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let tool = response["content"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|block| block["type"] == "tool_use"));
        assert!(
            tool.is_some(),
            "response did not contain an Anthropic tool_use block"
        );
        assert_eq!(tool.unwrap()["name"], "lookup_probe");
        println!(
            "native_tool_calling_supported=true model={}",
            response["model"].as_str().unwrap_or("unknown")
        );
    }
}

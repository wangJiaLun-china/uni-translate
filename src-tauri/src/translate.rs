use futures::StreamExt;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::client::{ChatMessage, ChatRequest, Chunk, Endpoint, OpenAiClient};
use crate::error::{AppError, AppResult};
use crate::history::NewEntry;
use crate::lang::{self, Language};
use crate::state::{AppState, Task};

/// Progress of one translation, pushed to the frontend over a channel.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StreamEvent {
    Started {
        detected_lang: Option<String>,
        target_lang: String,
    },
    Delta {
        text: String,
    },
    Done {
        full_text: String,
    },
    Error {
        message: String,
    },
}

pub fn build_messages(text: &str, target: &Language) -> Vec<ChatMessage> {
    let system = format!(
        "你是专业翻译引擎。自动识别原文语言，将其翻译为{}（{}）。\
         只输出译文，不要解释，不要添加引号或任何额外内容，保留原有格式与换行。",
        target.name, target.english
    );
    vec![
        ChatMessage {
            role: "system",
            content: system,
        },
        ChatMessage {
            role: "user",
            content: text.to_string(),
        },
    ]
}

/// Starts a streaming translation and returns its request id. A new request
/// cancels the one still running in the same window.
#[tauri::command]
pub async fn translate(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    text: String,
    target_lang: Option<String>,
    on_event: Channel<StreamEvent>,
) -> AppResult<String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::invalid("没有需要翻译的文字"));
    }
    let settings = state.config.get();
    if settings.model.is_empty() {
        return Err(AppError::invalid("请先在设置中填写模型名称"));
    }
    let detected = lang::detect(&text);
    let target_code = target_lang.as_deref().unwrap_or(&settings.target_lang);
    let target_code = lang::resolve_target(detected, target_code, &settings.secondary_lang);
    let target = *lang::find(target_code)
        .ok_or_else(|| AppError::invalid(format!("不支持的目标语言：{target_code}")))?;

    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        label: window.label().to_string(),
        endpoint: state.chat_endpoint(&settings),
        request: ChatRequest {
            model: settings.model.clone(),
            messages: build_messages(&text, &target),
            temperature: settings.temperature,
            stream: true,
            max_tokens: None,
        },
        text,
        detected,
        target,
        save_history: settings.history_enabled,
    };
    let id = job.id.clone();

    // Hold the lock while spawning so the task cannot finish (and clean up)
    // before it is registered.
    let mut tasks = state.tasks.lock().unwrap();
    if let Some(old) = tasks.remove(&job.label) {
        old.handle.abort();
    }
    let label = job.label.clone();
    let handle = tauri::async_runtime::spawn(run(app.clone(), job, on_event));
    tasks.insert(
        label,
        Task {
            id: id.clone(),
            handle,
        },
    );
    Ok(id)
}

#[tauri::command]
pub fn cancel_translation(state: State<'_, AppState>, id: String) {
    let mut tasks = state.tasks.lock().unwrap();
    let label = tasks
        .iter()
        .find(|(_, task)| task.id == id)
        .map(|(label, _)| label.clone());
    if let Some(task) = label.and_then(|label| tasks.remove(&label)) {
        task.handle.abort();
    }
}

struct Job {
    id: String,
    label: String,
    endpoint: Endpoint,
    request: ChatRequest,
    text: String,
    detected: Option<&'static str>,
    target: Language,
    save_history: bool,
}

async fn run(app: AppHandle, job: Job, channel: Channel<StreamEvent>) {
    let state = app.state::<AppState>();
    // Send errors only mean the window went away; there is nobody to tell.
    let _ = channel.send(StreamEvent::Started {
        detected_lang: job.detected.map(str::to_string),
        target_lang: job.target.code.to_string(),
    });

    match stream_chat(&state.client, &job.endpoint, &job.request, |text| {
        let _ = channel.send(StreamEvent::Delta { text });
    })
    .await
    {
        Ok(full_text) => {
            if job.save_history && !full_text.trim().is_empty() {
                let entry = NewEntry {
                    source_text: &job.text,
                    result_text: full_text.trim(),
                    source_lang: job.detected,
                    target_lang: job.target.code,
                    model: &job.request.model,
                };
                if let Err(err) = state.history.add(entry) {
                    log::warn!("failed to save history: {err}");
                }
            }
            let _ = channel.send(StreamEvent::Done { full_text });
        }
        Err(err) => {
            let _ = channel.send(StreamEvent::Error {
                message: err.to_string(),
            });
        }
    }

    let mut tasks = state.tasks.lock().unwrap();
    if tasks.get(&job.label).is_some_and(|task| task.id == job.id) {
        tasks.remove(&job.label);
    }
}

/// Runs a streaming chat request, calling `on_delta` for each piece of text,
/// and returns the full text.
async fn stream_chat(
    client: &OpenAiClient,
    endpoint: &Endpoint,
    request: &ChatRequest,
    mut on_delta: impl FnMut(String),
) -> AppResult<String> {
    let mut stream = client.chat_stream(endpoint, request).await?;
    let mut full_text = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk? {
            Chunk::Delta(text) => {
                full_text.push_str(&text);
                on_delta(text);
            }
            Chunk::Done => break,
        }
    }
    Ok(full_text)
}

/// Sends a tiny translation with unsaved settings from the settings page.
/// An empty `api_key` means "use the saved key".
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> AppResult<String> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err(AppError::invalid("请先填写模型名称"));
    }
    let endpoint = form_endpoint(&state, base_url, api_key);
    let request = ChatRequest {
        model,
        messages: build_messages("Hello, world!", lang::find("zh-CN").unwrap()),
        temperature: 0.0,
        stream: true,
        max_tokens: Some(32),
    };
    let text = stream_chat(&state.client, &endpoint, &request, |_| {}).await?;
    if text.trim().is_empty() {
        return Err(AppError::invalid("连接成功，但模型返回了空内容"));
    }
    Ok(text.trim().to_string())
}

#[tauri::command]
pub async fn list_models(
    state: State<'_, AppState>,
    base_url: String,
    api_key: Option<String>,
) -> AppResult<Vec<String>> {
    let endpoint = form_endpoint(&state, base_url, api_key);
    state.client.list_models(&endpoint).await
}

fn form_endpoint(state: &AppState, base_url: String, api_key: Option<String>) -> Endpoint {
    let api_key = api_key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .or_else(|| state.secrets.get(crate::secrets::KeyKind::Chat));
    Endpoint {
        base_url: crate::config::normalize_base_url(&base_url),
        api_key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_names_the_target_language() {
        let messages = build_messages("hello", lang::find("ja").unwrap());
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.contains("日语（Japanese）"));
        assert_eq!(messages[1].content, "hello");
    }

    #[test]
    fn stream_events_serialize_for_the_frontend() {
        let event = StreamEvent::Started {
            detected_lang: Some("en".into()),
            target_lang: "zh-CN".into(),
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"started","detectedLang":"en","targetLang":"zh-CN"}"#
        );
        let event = StreamEvent::Done {
            full_text: "好".into(),
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"done","fullText":"好"}"#
        );
    }
}

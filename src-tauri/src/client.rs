//! Minimal client for OpenAI-compatible APIs: streaming chat completions,
//! model listing and text-to-speech.

use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Mutex;
use std::time::Duration;

use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Serialize;
use serde_json::Value;

use crate::config::join_url;
use crate::error::{AppError, AppResult};

/// Where to send a request.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: &'static str,
    /// A string, or an array of parts for messages that carry an image.
    pub content: Value,
}

impl ChatMessage {
    pub fn text(role: &'static str, text: impl Into<String>) -> Self {
        Self {
            role,
            content: Value::String(text.into()),
        }
    }

    /// A user message with an instruction and one PNG image.
    pub fn with_png(text: &str, png: &[u8]) -> Self {
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        Self {
            role: "user",
            content: serde_json::json!([
                { "type": "text", "text": text },
                { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{data}") } },
            ]),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// `"none"` asks reasoning models to skip thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<&'static str>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// The same request without the optional tuning fields some models reject.
#[derive(Serialize)]
struct PlainChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

impl ChatRequest {
    fn has_tuning(&self) -> bool {
        self.temperature.is_some() || self.reasoning_effort.is_some()
    }

    fn plain(&self) -> PlainChatRequest<'_> {
        PlainChatRequest {
            model: &self.model,
            messages: &self.messages,
            stream: self.stream,
            max_tokens: self.max_tokens,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpeechRequest<'a> {
    pub model: &'a str,
    pub input: &'a str,
    pub voice: &'a str,
    pub speed: f32,
    pub response_format: &'static str,
}

/// One parsed server-sent event of a chat completion stream.
#[derive(Debug, PartialEq)]
pub enum Chunk {
    Delta(String),
    Done,
}

pub struct OpenAiClient {
    http: reqwest::Client,
    /// `base_url` + model pairs that rejected `temperature` or
    /// `reasoning_effort`; they get plain requests from then on.
    plain_models: Mutex<HashSet<String>>,
}

impl OpenAiClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("uni-translate/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            // Abort a stream that stalls; models can be slow to start, so be generous.
            .read_timeout(Duration::from_secs(90))
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            plain_models: Mutex::default(),
        }
    }

    /// The shared HTTP client, for services that are not OpenAI-compatible.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    fn request(
        &self,
        method: Method,
        endpoint: &Endpoint,
        path: &str,
    ) -> AppResult<RequestBuilder> {
        if endpoint.base_url.is_empty() {
            return Err(AppError::invalid("请先在设置中填写接口地址（Base URL）"));
        }
        let builder = self
            .http
            .request(method, join_url(&endpoint.base_url, path));
        Ok(match endpoint.api_key.as_deref() {
            Some(key) if !key.is_empty() => builder.bearer_auth(key),
            _ => builder,
        })
    }

    pub async fn chat_stream(
        &self,
        endpoint: &Endpoint,
        request: &ChatRequest,
    ) -> AppResult<impl Stream<Item = AppResult<Chunk>> + Send + Unpin> {
        let key = format!("{}\n{}", endpoint.base_url, request.model);
        let plain = !request.has_tuning() || self.plain_models.lock().unwrap().contains(&key);
        let builder = self.request(Method::POST, endpoint, "chat/completions")?;
        let mut response = if plain {
            builder.json(&request.plain()).send().await?
        } else {
            builder.json(request).send().await?
        };
        // Some models reject `temperature` (Kimi) or `reasoning_effort` (GLM)
        // with a 400. Retry once without them and remember the model.
        if !plain && response.status() == StatusCode::BAD_REQUEST {
            log::info!(
                "{} rejected tuning fields; retrying without them",
                request.model
            );
            self.plain_models.lock().unwrap().insert(key);
            response = self
                .request(Method::POST, endpoint, "chat/completions")?
                .json(&request.plain())
                .send()
                .await?;
        }
        let response = ensure_success(response).await?;
        Ok(Box::pin(chunk_stream(response.bytes_stream())))
    }

    pub async fn list_models(&self, endpoint: &Endpoint) -> AppResult<Vec<String>> {
        let response = self
            .request(Method::GET, endpoint, "models")?
            .send()
            .await?;
        let body: Value = ensure_success(response).await?.json().await?;
        let mut models: Vec<String> = body["data"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        models.sort();
        models.dedup();
        Ok(models)
    }

    /// Returns the encoded audio bytes.
    pub async fn speech(
        &self,
        endpoint: &Endpoint,
        request: &SpeechRequest<'_>,
    ) -> AppResult<Vec<u8>> {
        let response = self
            .request(Method::POST, endpoint, "audio/speech")?
            .timeout(Duration::from_secs(120))
            .json(request)
            .send()
            .await?;
        let response = ensure_success(response).await?;
        let is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("json"));
        let bytes = response.bytes().await?;
        if is_json {
            // Some servers answer 200 with a JSON error instead of audio.
            let message = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| api_error_message(&v))
                .unwrap_or_else(|| "语音接口没有返回音频数据".into());
            return Err(AppError::Stream(message));
        }
        Ok(bytes.to_vec())
    }
}

/// Turns a non-2xx response into an error carrying the server's own message.
async fn ensure_success(response: Response) -> AppResult<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| api_error_message(&v))
        .unwrap_or_else(|| truncate(body.trim(), 300));
    let message = if message.is_empty() {
        status.canonical_reason().unwrap_or("未知错误").to_string()
    } else {
        message
    };
    Err(AppError::Api {
        status: status.as_u16(),
        message,
    })
}

/// Extracts the error message from the error shapes used by OpenAI-compatible
/// servers: `{"error":{"message":..}}`, `{"error":".."}` or `{"message":..}`.
pub fn api_error_message(value: &Value) -> Option<String> {
    let error = &value["error"];
    if let Some(message) = error["message"].as_str().or_else(|| error.as_str()) {
        return Some(message.to_string());
    }
    if value.get("choices").is_none() {
        return value["message"].as_str().map(str::to_string);
    }
    None
}

/// Parses an SSE byte stream into chat completion chunks.
pub fn chunk_stream<S, B, E>(bytes: S) -> impl Stream<Item = AppResult<Chunk>>
where
    S: Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
    E: Display,
{
    bytes.eventsource().filter_map(|event| async move {
        match event {
            Ok(event) => parse_sse_data(&event.data).transpose(),
            Err(err) => Some(Err(AppError::Stream(err.to_string()))),
        }
    })
}

/// Parses the `data:` payload of one event. Keep-alives, empty deltas and
/// non-JSON lines yield `None`.
pub fn parse_sse_data(data: &str) -> AppResult<Option<Chunk>> {
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(Some(Chunk::Done));
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return Ok(None);
    };
    if let Some(message) = api_error_message(&value) {
        return Err(AppError::Stream(message));
    }
    Ok(value["choices"][0]["delta"]["content"]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(|text| Chunk::Delta(text.to_string())))
}

/// Drops a leading `<think>…</think>` block, which some models (e.g.
/// MiniMax-M3) write into the answer instead of `reasoning_content`.
/// Tags may be split across stream chunks.
#[derive(Debug, Default)]
pub struct ThinkFilter {
    state: ThinkState,
    buffer: String,
}

#[derive(Debug, Default, PartialEq)]
enum ThinkState {
    /// Not sure yet whether the answer opens with `<think>`.
    #[default]
    Start,
    Inside,
    /// Just past `</think>`: skip whitespace before the answer.
    Trim,
    Pass,
}

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

impl ThinkFilter {
    /// Returns the part of `chunk` that belongs to the answer (possibly empty).
    pub fn push(&mut self, chunk: &str) -> String {
        match self.state {
            ThinkState::Pass => chunk.to_string(),
            ThinkState::Trim => {
                let answer = chunk.trim_start();
                if !answer.is_empty() {
                    self.state = ThinkState::Pass;
                }
                answer.to_string()
            }
            ThinkState::Start => {
                self.buffer.push_str(chunk);
                let head = self.buffer.trim_start();
                if head.is_empty()
                    || (head.len() < THINK_OPEN.len() && THINK_OPEN.starts_with(head))
                {
                    return String::new();
                }
                if let Some(rest) = head.strip_prefix(THINK_OPEN) {
                    let rest = rest.to_string();
                    self.state = ThinkState::Inside;
                    self.buffer.clear();
                    return self.push(&rest);
                }
                self.state = ThinkState::Pass;
                std::mem::take(&mut self.buffer)
            }
            ThinkState::Inside => {
                self.buffer.push_str(chunk);
                if let Some(end) = self.buffer.find(THINK_CLOSE) {
                    let rest = self.buffer[end + THINK_CLOSE.len()..].to_string();
                    self.buffer.clear();
                    self.state = ThinkState::Trim;
                    return self.push(&rest);
                }
                // Keep only enough to spot a closing tag split across chunks.
                let keep = self
                    .buffer
                    .char_indices()
                    .rev()
                    .nth(THINK_CLOSE.len() - 2)
                    .map_or(0, |(i, _)| i);
                self.buffer.drain(..keep);
                String::new()
            }
        }
    }

    /// Text held back at the end of the stream: a lone prefix such as `<th`
    /// that turned out to be the whole answer.
    pub fn finish(&mut self) -> String {
        match self.state {
            ThinkState::Start => std::mem::take(&mut self.buffer),
            _ => String::new(),
        }
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}…", &text[..idx]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    fn delta(text: &str) -> String {
        serde_json::json!({ "choices": [{ "index": 0, "delta": { "content": text } }] }).to_string()
    }

    #[test]
    fn parses_delta_done_and_noise() {
        assert_eq!(
            parse_sse_data(&delta("你好")).unwrap(),
            Some(Chunk::Delta("你好".into()))
        );
        assert_eq!(parse_sse_data(" [DONE] ").unwrap(), Some(Chunk::Done));
        assert_eq!(parse_sse_data("").unwrap(), None);
        assert_eq!(parse_sse_data(": keep-alive").unwrap(), None);
        // Role-only first chunk and reasoning chunks carry no content.
        assert_eq!(
            parse_sse_data(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#).unwrap(),
            None
        );
        assert_eq!(
            parse_sse_data(r#"{"choices":[{"delta":{"content":null,"reasoning_content":"hm"}}]}"#)
                .unwrap(),
            None
        );
    }

    #[test]
    fn reports_in_stream_errors() {
        let err =
            parse_sse_data(r#"{"error":{"message":"quota exceeded","code":"429"}}"#).unwrap_err();
        assert_eq!(err.to_string(), "接口返回错误：quota exceeded");
    }

    #[test]
    fn error_message_shapes() {
        let v = |s: &str| serde_json::from_str::<Value>(s).unwrap();
        assert_eq!(
            api_error_message(&v(r#"{"error":{"message":"bad key"}}"#)).as_deref(),
            Some("bad key")
        );
        assert_eq!(
            api_error_message(&v(r#"{"error":"bad key"}"#)).as_deref(),
            Some("bad key")
        );
        assert_eq!(
            api_error_message(&v(r#"{"message":"bad key"}"#)).as_deref(),
            Some("bad key")
        );
        assert_eq!(
            api_error_message(&v(r#"{"choices":[],"message":"x"}"#)),
            None
        );
    }

    #[tokio::test]
    async fn stream_survives_split_packets() {
        let body = format!(
            "data: {}\n\n: ping\n\ndata: {}\n\ndata: [DONE]\n\n",
            delta("Hello"),
            delta("，世界")
        );
        // Split into awkward 7-byte packets, including through multi-byte UTF-8.
        let packets: Vec<Result<Vec<u8>, std::io::Error>> =
            body.as_bytes().chunks(7).map(|c| Ok(c.to_vec())).collect();
        let chunks: Vec<Chunk> = chunk_stream(stream::iter(packets))
            .map(Result::unwrap)
            .collect()
            .await;
        assert_eq!(
            chunks,
            vec![
                Chunk::Delta("Hello".into()),
                Chunk::Delta("，世界".into()),
                Chunk::Done
            ]
        );
    }

    /// Serves one canned HTTP response on a local port and returns its base URL.
    async fn serve_once(status: &'static str, content_type: &'static str, body: String) -> String {
        let reply = crate::test_server::Reply::new(status, content_type, body);
        let (base, _) = crate::test_server::serve(vec![reply]).await;
        format!("{base}/v1/")
    }

    fn endpoint(base_url: String) -> Endpoint {
        Endpoint {
            base_url,
            api_key: Some("sk-test".into()),
        }
    }

    fn chat_request() -> ChatRequest {
        ChatRequest {
            model: "m".into(),
            messages: vec![],
            temperature: None,
            reasoning_effort: None,
            stream: true,
            max_tokens: None,
        }
    }

    #[tokio::test]
    async fn chat_stream_over_http() {
        let body = format!(
            "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
            delta("你"),
            delta("好")
        );
        let base = serve_once("200 OK", "text/event-stream", body).await;
        let client = OpenAiClient::new();
        let stream = client
            .chat_stream(&endpoint(base), &chat_request())
            .await
            .unwrap();
        let chunks: Vec<Chunk> = stream.map(Result::unwrap).collect().await;
        assert_eq!(
            chunks,
            vec![
                Chunk::Delta("你".into()),
                Chunk::Delta("好".into()),
                Chunk::Done
            ]
        );
    }

    #[tokio::test]
    async fn http_errors_carry_the_server_message() {
        let body = r#"{"error":{"message":"Invalid API key","type":"auth"}}"#.to_string();
        let base = serve_once("401 Unauthorized", "application/json", body).await;
        let client = OpenAiClient::new();
        let err = match client.chat_stream(&endpoint(base), &chat_request()).await {
            Err(err) => err,
            Ok(_) => panic!("expected an error"),
        };
        assert_eq!(err.to_string(), "接口返回错误（HTTP 401）：Invalid API key");
    }

    #[tokio::test]
    async fn lists_models_sorted() {
        let body = r#"{"object":"list","data":[{"id":"b"},{"id":"a"},{"id":"b"}]}"#.to_string();
        let base = serve_once("200 OK", "application/json", body).await;
        let models = OpenAiClient::new()
            .list_models(&endpoint(base))
            .await
            .unwrap();
        assert_eq!(models, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn speech_returns_audio_or_json_error() {
        let speech = SpeechRequest {
            model: "tts-1",
            input: "hi",
            voice: "alloy",
            speed: 1.0,
            response_format: "mp3",
        };
        let base = serve_once("200 OK", "audio/mpeg", "ID3fake".to_string()).await;
        let audio = OpenAiClient::new()
            .speech(&endpoint(base), &speech)
            .await
            .unwrap();
        assert_eq!(audio, b"ID3fake");

        let base = serve_once(
            "200 OK",
            "application/json",
            r#"{"error":"no tts"}"#.to_string(),
        )
        .await;
        let err = OpenAiClient::new()
            .speech(&endpoint(base), &speech)
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "接口返回错误：no tts");
    }

    #[test]
    fn missing_base_url_is_explained() {
        let client = OpenAiClient::new();
        let err = client
            .request(Method::GET, &endpoint(String::new()), "models")
            .unwrap_err();
        assert!(err.to_string().contains("接口地址"));
    }

    fn filter_all(chunks: &[&str]) -> String {
        let mut filter = ThinkFilter::default();
        let mut out: String = chunks.iter().map(|c| filter.push(c)).collect();
        out.push_str(&filter.finish());
        out
    }

    #[test]
    fn think_filter_drops_a_leading_think_block() {
        assert_eq!(filter_all(&["<think>plan</think>\n你好"]), "你好");
        // Tags split across chunks, including multi-byte text inside.
        assert_eq!(
            filter_all(&[
                "\n<thi",
                "nk>想一想，翻译",
                "成中文</th",
                "ink>",
                "\n\n早上",
                "好"
            ]),
            "早上好"
        );
        assert_eq!(
            filter_all(&["Hello", " <think>is text</think>"]),
            "Hello <think>is text</think>"
        );
        assert_eq!(filter_all(&["<", "b>bold</b>"]), "<b>bold</b>");
        assert_eq!(filter_all(&["<th"]), "<th");
        assert_eq!(filter_all(&["<think>never closed"]), "");
    }

    #[test]
    fn image_messages_use_data_urls() {
        let message = ChatMessage::with_png("read", b"PNG");
        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["content"][0]["text"], "read");
        assert_eq!(
            json["content"][1]["image_url"]["url"],
            "data:image/png;base64,UE5H"
        );
    }

    #[tokio::test]
    async fn retries_without_tuning_after_a_400_and_remembers() {
        use crate::test_server::{serve, Reply};
        let ok = || {
            Reply::new(
                "200 OK",
                "text/event-stream",
                format!("data: {}\n\ndata: [DONE]\n\n", delta("好")),
            )
        };
        let rejected = Reply::new(
            "400 Bad Request",
            "application/json",
            r#"{"error":{"message":"temperature not allowed"}}"#,
        );
        let (base, requests) = serve(vec![rejected, ok(), ok()]).await;
        let client = OpenAiClient::new();
        let endpoint = endpoint(format!("{base}/v1"));
        let request = ChatRequest {
            temperature: Some(0.3),
            reasoning_effort: Some("none"),
            ..chat_request()
        };
        for _ in 0..2 {
            let chunks: Vec<Chunk> = client
                .chat_stream(&endpoint, &request)
                .await
                .unwrap()
                .map(Result::unwrap)
                .collect()
                .await;
            assert_eq!(chunks[0], Chunk::Delta("好".into()));
        }
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].contains(r#""temperature":0.3"#));
        assert!(requests[0].contains(r#""reasoning_effort":"none""#));
        for later in &requests[1..] {
            assert!(!later.contains("temperature") && !later.contains("reasoning_effort"));
        }
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("翻译工具", 2), "翻译…");
        assert_eq!(truncate("ab", 5), "ab");
    }
}

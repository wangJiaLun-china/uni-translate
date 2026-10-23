//! Minimal client for OpenAI-compatible APIs: streaming chat completions,
//! model listing and text-to-speech.

use std::fmt::Display;
use std::time::Duration;

use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use reqwest::{Method, RequestBuilder, Response};
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
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
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
        Self { http }
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
        let response = self
            .request(Method::POST, endpoint, "chat/completions")?
            .json(request)
            .send()
            .await?;
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
            temperature: 0.0,
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

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("翻译工具", 2), "翻译…");
        assert_eq!(truncate("ab", 5), "ab");
    }
}

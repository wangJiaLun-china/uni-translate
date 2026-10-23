//! Baidu AI Cloud short-text speech synthesis (`text2audio`).
//! https://cloud.baidu.com/doc/SPEECH/s/mlbxh7xie

use std::sync::Mutex;
use std::time::{Duration, Instant};

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::Value;

use crate::error::{AppError, AppResult};

const TOKEN_URL: &str = "https://aip.baidubce.com/oauth/2.0/token";
const TTS_URL: &str = "https://tsn.baidu.com/text2audio";
/// Identifies this client to Baidu (at most 60 characters).
const CUID: &str = "uni-translate";
/// Refresh a token this long before it expires.
const EXPIRY_MARGIN: Duration = Duration::from_secs(300);
/// `err_no` for an invalid or expired access token.
const TOKEN_REJECTED: i64 = 502;

/// Baidu limits one request to 1024 GBK bytes; at most two bytes per
/// character, 500 characters always fit.
pub const MAX_SEGMENT_CHARS: usize = 500;
const _: () = assert!(MAX_SEGMENT_CHARS * 2 <= 1024);

struct CachedToken {
    api_key: String,
    value: String,
    refresh_at: Instant,
}

pub struct BaiduTts {
    token_url: String,
    tts_url: String,
    token: Mutex<Option<CachedToken>>,
}

enum Synthesis {
    Audio(Vec<u8>),
    TokenRejected,
}

/// Maps the shared speed multiple (1.0 = normal) to Baidu's 0–15 scale (5 = normal).
pub fn speed(multiple: f32) -> u8 {
    (multiple * 5.0).round().clamp(0.0, 15.0) as u8
}

impl BaiduTts {
    pub fn new() -> Self {
        Self::with_urls(TOKEN_URL, TTS_URL)
    }

    fn with_urls(token_url: &str, tts_url: &str) -> Self {
        Self {
            token_url: token_url.to_string(),
            tts_url: tts_url.to_string(),
            token: Mutex::default(),
        }
    }

    /// Forgets the cached access token, e.g. after the keys change.
    pub fn clear_token(&self) {
        *self.token.lock().unwrap() = None;
    }

    /// Returns MP3 audio for `text` spoken by voice `per` at Baidu speed `spd`.
    pub async fn synthesize(
        &self,
        http: &reqwest::Client,
        api_key: &str,
        secret_key: &str,
        text: &str,
        per: u32,
        spd: u8,
    ) -> AppResult<Vec<u8>> {
        let token = self.token(http, api_key, secret_key).await?;
        if let Synthesis::Audio(audio) = self.request_audio(http, &token, text, per, spd).await? {
            return Ok(audio);
        }
        // The cached token was revoked or expired early: fetch a new one once.
        self.clear_token();
        let token = self.token(http, api_key, secret_key).await?;
        match self.request_audio(http, &token, text, per, spd).await? {
            Synthesis::Audio(audio) => Ok(audio),
            Synthesis::TokenRejected => {
                Err(AppError::invalid("百度语音拒绝了新获取的 access token"))
            }
        }
    }

    async fn token(
        &self,
        http: &reqwest::Client,
        api_key: &str,
        secret_key: &str,
    ) -> AppResult<String> {
        if let Some(cached) = self.token.lock().unwrap().as_ref() {
            if cached.api_key == api_key && Instant::now() < cached.refresh_at {
                return Ok(cached.value.clone());
            }
        }
        let response = http
            .post(&self.token_url)
            .query(&[
                ("grant_type", "client_credentials"),
                ("client_id", api_key),
                ("client_secret", secret_key),
            ])
            .send()
            .await
            .map_err(hide_url)?;
        let status = response.status().as_u16();
        let body: Value = response.json().await.map_err(hide_url)?;
        let Some(value) = body["access_token"].as_str() else {
            return Err(token_error(status, &body));
        };
        let lifetime = Duration::from_secs(body["expires_in"].as_u64().unwrap_or(86_400));
        let token = value.to_string();
        *self.token.lock().unwrap() = Some(CachedToken {
            api_key: api_key.to_string(),
            value: token.clone(),
            refresh_at: Instant::now() + lifetime.saturating_sub(EXPIRY_MARGIN),
        });
        Ok(token)
    }

    async fn request_audio(
        &self,
        http: &reqwest::Client,
        token: &str,
        text: &str,
        per: u32,
        spd: u8,
    ) -> AppResult<Synthesis> {
        let form = synthesis_form(text, token, per, spd);
        let response = http
            .post(&self.tts_url)
            .timeout(Duration::from_secs(60))
            .form(&form)
            .send()
            .await?;
        let is_audio = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("audio"));
        let bytes = response.bytes().await?;
        if is_audio {
            return Ok(Synthesis::Audio(bytes.to_vec()));
        }
        let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
        let err_no = body["err_no"].as_i64();
        if err_no == Some(TOKEN_REJECTED) {
            return Ok(Synthesis::TokenRejected);
        }
        let message = body["err_msg"].as_str().unwrap_or("没有返回音频");
        Err(AppError::invalid(match err_no {
            Some(code) => format!("百度语音合成失败（{code}）：{message}"),
            None => format!("百度语音合成失败：{message}"),
        }))
    }
}

/// Form fields for `text2audio`. Baidu recommends URL-encoding `tex` twice:
/// once here, once more when the form is encoded.
fn synthesis_form(text: &str, token: &str, per: u32, spd: u8) -> Vec<(&'static str, String)> {
    vec![
        (
            "tex",
            utf8_percent_encode(text, NON_ALPHANUMERIC).to_string(),
        ),
        ("tok", token.to_string()),
        ("cuid", CUID.to_string()),
        ("ctp", "1".to_string()),
        ("lan", "zh".to_string()),
        ("spd", spd.to_string()),
        ("pit", "5".to_string()),
        ("vol", "5".to_string()),
        ("per", per.to_string()),
        ("aue", "3".to_string()),
    ]
}

/// The token URL carries the secret key, so keep it out of error messages.
fn hide_url(err: reqwest::Error) -> AppError {
    AppError::Http(err.without_url())
}

fn token_error(status: u16, body: &Value) -> AppError {
    let code = body["error"].as_str().unwrap_or_default();
    if code == "invalid_client" {
        return AppError::invalid("百度语音的 API Key 或 Secret Key 不正确");
    }
    let detail = body["error_description"]
        .as_str()
        .filter(|d| !d.is_empty())
        .unwrap_or(code);
    AppError::Api {
        status,
        message: format!("获取百度 access token 失败：{detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{serve, Reply};

    fn token_reply(token: &str) -> Reply {
        let body = format!(
            r#"{{"access_token":"{token}","expires_in":2592000,"scope":"audio_tts_post"}}"#
        );
        Reply::new("200 OK", "application/json", body)
    }

    fn audio_reply() -> Reply {
        Reply::new("200 OK", "audio/mp3", b"ID3audio".to_vec())
    }

    async fn tts_with(
        token_replies: Vec<Reply>,
        audio_replies: Vec<Reply>,
    ) -> (
        BaiduTts,
        crate::test_server::Requests,
        crate::test_server::Requests,
    ) {
        let (token_url, token_requests) = serve(token_replies).await;
        let (tts_url, tts_requests) = serve(audio_replies).await;
        let tts = BaiduTts::with_urls(
            &format!("{token_url}/oauth/2.0/token"),
            &format!("{tts_url}/text2audio"),
        );
        (tts, token_requests, tts_requests)
    }

    #[test]
    fn maps_speed() {
        assert_eq!(speed(1.0), 5);
        assert_eq!(speed(0.5), 3);
        assert_eq!(speed(2.0), 10);
        assert_eq!(speed(9.0), 15);
    }

    #[test]
    fn encodes_text_twice() {
        let form = synthesis_form("你好 a+b", "tok", 0, 5);
        let encoded = serde_urlencoded::to_string(&form).unwrap();
        // "你" is E4 BD A0; the second pass turns each % into %25.
        assert!(encoded.starts_with("tex=%25E4%25BD%25A0"), "{encoded}");
        assert!(encoded.contains("%2520a%252Bb"), "{encoded}");
        assert!(encoded.contains("&ctp=1&lan=zh&spd=5&pit=5&vol=5&per=0&aue=3"));
    }

    #[tokio::test]
    async fn fetches_a_token_once_then_reuses_it() {
        let (tts, token_requests, tts_requests) =
            tts_with(vec![token_reply("t1")], vec![audio_reply(), audio_reply()]).await;
        let http = reqwest::Client::new();
        for _ in 0..2 {
            let audio = tts
                .synthesize(&http, "ak", "sk", "你好", 0, 5)
                .await
                .unwrap();
            assert_eq!(audio, b"ID3audio");
        }
        let token_requests = token_requests.lock().unwrap();
        assert_eq!(token_requests.len(), 1);
        assert!(token_requests[0].starts_with(
            "POST /oauth/2.0/token?grant_type=client_credentials&client_id=ak&client_secret=sk "
        ));
        let tts_requests = tts_requests.lock().unwrap();
        assert_eq!(tts_requests.len(), 2);
        assert!(tts_requests[0].contains("tok=t1&cuid=uni-translate"));
    }

    #[tokio::test]
    async fn retries_once_with_a_fresh_token() {
        let rejected = Reply::new(
            "200 OK",
            "application/json",
            r#"{"err_no":502,"err_msg":"access token invalid or no longer valid","sn":"x","idx":1}"#,
        );
        let (tts, token_requests, tts_requests) = tts_with(
            vec![token_reply("old"), token_reply("new")],
            vec![rejected, audio_reply()],
        )
        .await;
        let audio = tts
            .synthesize(&reqwest::Client::new(), "ak", "sk", "hi", 0, 5)
            .await
            .unwrap();
        assert_eq!(audio, b"ID3audio");
        assert_eq!(token_requests.lock().unwrap().len(), 2);
        assert!(tts_requests.lock().unwrap()[1].contains("tok=new"));
    }

    #[tokio::test]
    async fn reports_wrong_keys_plainly() {
        let reply = Reply::new(
            "401 Unauthorized",
            "application/json",
            r#"{"error":"invalid_client","error_description":"unknown client id"}"#,
        );
        let (tts, _, _) = tts_with(vec![reply], vec![]).await;
        let err = tts
            .synthesize(&reqwest::Client::new(), "bad", "bad", "hi", 0, 5)
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "百度语音的 API Key 或 Secret Key 不正确");
    }

    #[tokio::test]
    async fn network_errors_do_not_leak_the_secret() {
        // Nothing listens on port 9 of localhost: the request fails to connect.
        let tts = BaiduTts::with_urls("http://127.0.0.1:9/token", "http://127.0.0.1:9/tts");
        let err = tts
            .synthesize(&reqwest::Client::new(), "ak", "top-secret", "hi", 0, 5)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("网络请求失败"), "{err}");
        assert!(!err.contains("top-secret"), "{err}");
    }

    #[tokio::test]
    async fn reports_synthesis_errors() {
        let reply = Reply::new(
            "200 OK",
            "application/json",
            r#"{"err_no":501,"err_msg":"param per invalid"}"#,
        );
        let (tts, _, _) = tts_with(vec![token_reply("t")], vec![reply]).await;
        let err = tts
            .synthesize(&reqwest::Client::new(), "ak", "sk", "hi", 99999, 5)
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "百度语音合成失败（501）：param per invalid"
        );
    }

    /// Calls the real service. Run with keys from the environment:
    /// `BAIDU_API_KEY=… BAIDU_SECRET_KEY=… cargo test live_baidu -- --ignored`
    #[tokio::test]
    #[ignore = "needs Baidu credentials and network access"]
    async fn live_baidu() {
        let api_key = std::env::var("BAIDU_API_KEY").expect("BAIDU_API_KEY");
        let secret_key = std::env::var("BAIDU_SECRET_KEY").expect("BAIDU_SECRET_KEY");
        let audio = BaiduTts::new()
            .synthesize(
                &reqwest::Client::new(),
                &api_key,
                &secret_key,
                "你好，这是 uni-translate 的语音测试。Hello + world!",
                0,
                speed(1.0),
            )
            .await
            .unwrap();
        // MP3 data starts with an ID3 tag or an MPEG frame sync.
        assert!(
            audio.starts_with(b"ID3") || audio.starts_with(&[0xff]),
            "not MP3"
        );
        if let Ok(path) = std::env::var("BAIDU_AUDIO_OUT") {
            std::fs::write(path, &audio).unwrap();
        }
    }
}

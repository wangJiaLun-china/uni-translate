use tauri::ipc::Response;
use tauri::State;

use crate::client::SpeechRequest;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Longest piece sent in one `/audio/speech` call. Well below the usual 4096
/// limit so the first piece comes back quickly; later pieces are prefetched.
const MAX_SEGMENT_CHARS: usize = 1000;

/// Synthesizes `text` and returns the MP3 bytes as a raw binary response.
#[tauri::command]
pub async fn speak(state: State<'_, AppState>, text: String) -> AppResult<Response> {
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::invalid("没有需要朗读的文字"));
    }
    let settings = state.config.get();
    if settings.tts.model.is_empty() || settings.tts.voice.is_empty() {
        return Err(AppError::invalid("请先在设置中填写语音模型和音色"));
    }
    let endpoint = state.tts_endpoint(&settings);
    let request = SpeechRequest {
        model: &settings.tts.model,
        input: text,
        voice: &settings.tts.voice,
        speed: settings.tts.speed,
        response_format: "mp3",
    };
    let audio = state.client.speech(&endpoint, &request).await?;
    Ok(Response::new(audio))
}

/// Splits text into pieces the frontend speaks one after another.
#[tauri::command]
pub fn split_tts_text(text: String) -> Vec<String> {
    split_text(&text, MAX_SEGMENT_CHARS)
}

/// Packs whole sentences into segments of at most `max_chars` characters,
/// hard-splitting only sentences that are longer than that on their own.
pub fn split_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;

    for sentence in sentences(text) {
        let len = sentence.chars().count();
        if current_len + len > max_chars {
            flush(&mut current, &mut segments);
            current_len = 0;
        }
        if len > max_chars {
            let chars: Vec<char> = sentence.chars().collect();
            for piece in chars.chunks(max_chars) {
                current.extend(piece);
                flush(&mut current, &mut segments);
            }
            continue;
        }
        current.push_str(sentence);
        current_len += len;
    }
    flush(&mut current, &mut segments);
    segments
}

fn flush(current: &mut String, segments: &mut Vec<String>) {
    let segment = current.trim();
    if !segment.is_empty() {
        segments.push(segment.to_string());
    }
    current.clear();
}

/// Splits after sentence-ending punctuation and newlines, keeping the
/// punctuation. A `.` only ends a sentence when followed by whitespace, so
/// numbers like `3.14` stay intact.
fn sentences(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        let next = chars.peek().map(|&(_, n)| n);
        let ends = match c {
            '。' | '！' | '？' | '；' | '!' | '?' | ';' | '\n' => true,
            '.' => next.is_none_or(char::is_whitespace),
            _ => false,
        };
        if ends {
            let end = idx + c.len_utf8();
            result.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        result.push(&text[start..]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_segment() {
        assert_eq!(split_text("  Hello. World!  ", 100), vec!["Hello. World!"]);
        assert!(split_text("   ", 100).is_empty());
    }

    #[test]
    fn packs_sentences_without_breaking_them() {
        let segments = split_text("第一句。第二句！第三句？", 8);
        assert_eq!(segments, vec!["第一句。第二句！", "第三句？"]);
        assert_eq!(
            split_text("Pi is 3.14. Done.", 12),
            vec!["Pi is 3.14.", "Done."]
        );
    }

    #[test]
    fn hard_splits_overlong_sentences() {
        let text = "a".repeat(25);
        let segments = split_text(&text, 10);
        assert_eq!(
            segments.iter().map(String::len).collect::<Vec<_>>(),
            vec![10, 10, 5]
        );
    }

    #[test]
    fn segments_respect_the_limit() {
        let text = "这是一个很长的句子，用来测试切分。".repeat(200);
        for segment in split_text(&text, MAX_SEGMENT_CHARS) {
            assert!(segment.chars().count() <= MAX_SEGMENT_CHARS);
        }
    }
}

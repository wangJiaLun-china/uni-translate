//! Supported target languages and local source-language detection.

use std::sync::LazyLock;

use serde::Serialize;
use whatlang::{Detector, Lang, Script};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Language {
    pub code: &'static str,
    /// Display name in the UI.
    pub name: &'static str,
    /// Name used in the prompt.
    pub english: &'static str,
}

pub const LANGUAGES: &[Language] = &[
    Language {
        code: "zh-CN",
        name: "简体中文",
        english: "Simplified Chinese",
    },
    Language {
        code: "zh-TW",
        name: "繁體中文",
        english: "Traditional Chinese",
    },
    Language {
        code: "en",
        name: "英语",
        english: "English",
    },
    Language {
        code: "ja",
        name: "日语",
        english: "Japanese",
    },
    Language {
        code: "ko",
        name: "韩语",
        english: "Korean",
    },
    Language {
        code: "fr",
        name: "法语",
        english: "French",
    },
    Language {
        code: "de",
        name: "德语",
        english: "German",
    },
    Language {
        code: "es",
        name: "西班牙语",
        english: "Spanish",
    },
    Language {
        code: "ru",
        name: "俄语",
        english: "Russian",
    },
    Language {
        code: "pt",
        name: "葡萄牙语",
        english: "Portuguese",
    },
    Language {
        code: "it",
        name: "意大利语",
        english: "Italian",
    },
    Language {
        code: "ar",
        name: "阿拉伯语",
        english: "Arabic",
    },
    Language {
        code: "th",
        name: "泰语",
        english: "Thai",
    },
    Language {
        code: "vi",
        name: "越南语",
        english: "Vietnamese",
    },
    Language {
        code: "id",
        name: "印尼语",
        english: "Indonesian",
    },
];

pub fn find(code: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|lang| lang.code == code)
}

/// `zh-CN` and `zh-TW` both count as `zh` when comparing with the source.
fn base_code(code: &str) -> &str {
    code.split('-').next().unwrap_or(code)
}

/// Languages we can name; limiting the detector to them improves accuracy.
const DETECTABLE: &[Lang] = &[
    Lang::Cmn,
    Lang::Eng,
    Lang::Jpn,
    Lang::Kor,
    Lang::Fra,
    Lang::Deu,
    Lang::Spa,
    Lang::Rus,
    Lang::Por,
    Lang::Ita,
    Lang::Ara,
    Lang::Tha,
    Lang::Vie,
    Lang::Ind,
];

/// Below this confidence a guess is too likely to be wrong to act on.
const MIN_CONFIDENCE: f64 = 0.2;

static DETECTOR: LazyLock<Detector> =
    LazyLock::new(|| Detector::with_allowlist(DETECTABLE.to_vec()));

/// Detects the source language and returns its base code (`zh`, `en`, ...).
/// Returns `None` when unsure, which is common for very short text.
pub fn detect(text: &str) -> Option<&'static str> {
    if let Some(info) = DETECTOR.detect(text) {
        if info.is_reliable() || info.confidence() >= MIN_CONFIDENCE {
            if let Some(code) = lang_code(info.lang()) {
                return Some(code);
            }
        }
    }
    // Scripts that belong to a single language are safe even for short text.
    match whatlang::detect_script(text)? {
        Script::Hiragana | Script::Katakana => Some("ja"),
        Script::Hangul => Some("ko"),
        Script::Thai => Some("th"),
        Script::Mandarin if !has_kana(text) => Some("zh"),
        _ => None,
    }
}

fn has_kana(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}'))
}

fn lang_code(lang: Lang) -> Option<&'static str> {
    Some(match lang {
        Lang::Cmn => "zh",
        Lang::Eng => "en",
        Lang::Jpn => "ja",
        Lang::Kor => "ko",
        Lang::Fra => "fr",
        Lang::Deu => "de",
        Lang::Spa => "es",
        Lang::Rus => "ru",
        Lang::Por => "pt",
        Lang::Ita => "it",
        Lang::Ara => "ar",
        Lang::Tha => "th",
        Lang::Vie => "vi",
        Lang::Ind => "id",
        _ => return None,
    })
}

/// Picks the language to translate into: `target`, unless the source already
/// is that language, in which case `secondary`.
pub fn resolve_target<'a>(detected: Option<&str>, target: &'a str, secondary: &'a str) -> &'a str {
    match detected {
        Some(source) if source == base_code(target) && base_code(secondary) != source => secondary,
        _ => target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_languages() {
        assert_eq!(detect("今天天气很好，我们一起去公园散步吧。"), Some("zh"));
        assert_eq!(detect("你好"), Some("zh"));
        assert_eq!(
            detect("The quick brown fox jumps over the lazy dog near the river bank."),
            Some("en")
        );
        assert_eq!(detect("こんにちは"), Some("ja"));
        assert_eq!(detect("東京は晴れです"), Some("ja"));
        assert_eq!(detect("안녕하세요"), Some("ko"));
        assert_eq!(
            detect("Bonjour, comment allez-vous aujourd'hui ?"),
            Some("fr")
        );
        assert_eq!(detect("Привет, как дела?"), Some("ru"));
        assert_eq!(detect("123 456"), None);
    }

    #[test]
    fn swaps_to_secondary_when_source_is_target() {
        assert_eq!(resolve_target(Some("zh"), "zh-CN", "en"), "en");
        assert_eq!(resolve_target(Some("en"), "zh-CN", "en"), "zh-CN");
        assert_eq!(resolve_target(None, "zh-CN", "en"), "zh-CN");
        assert_eq!(resolve_target(Some("en"), "en", "zh-CN"), "zh-CN");
        // Never swap into the language the text is already in.
        assert_eq!(resolve_target(Some("zh"), "zh-CN", "zh-TW"), "zh-CN");
    }

    #[test]
    fn every_language_is_findable() {
        for lang in LANGUAGES {
            assert_eq!(find(lang.code).unwrap().code, lang.code);
        }
        assert!(find("xx").is_none());
    }
}

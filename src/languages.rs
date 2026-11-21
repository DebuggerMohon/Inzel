//! 39 supported languages: (translate_code, google_stt_code, English name, native name).

pub const LANGS: &[(&str, &str, &str, &str)] = &[
    ("en", "en-US", "English", "English"),
    ("bn", "bn-BD", "Bangla", "বাংলা"),
    ("hi", "hi-IN", "Hindi", "हिन्दी"),
    ("ur", "ur-PK", "Urdu", "اردو"),
    ("ar", "ar-SA", "Arabic", "العربية"),
    ("es", "es-ES", "Spanish", "Español"),
    ("fr", "fr-FR", "French", "Français"),
    ("de", "de-DE", "German", "Deutsch"),
    ("pt", "pt-BR", "Portuguese", "Português"),
    ("ru", "ru-RU", "Russian", "Русский"),
    ("zh-CN", "zh-CN", "Chinese", "中文"),
    ("ja", "ja-JP", "Japanese", "日本語"),
    ("ko", "ko-KR", "Korean", "한국어"),
    ("id", "id-ID", "Indonesian", "Bahasa Indonesia"),
    ("ms", "ms-MY", "Malay", "Bahasa Melayu"),
    ("tr", "tr-TR", "Turkish", "Türkçe"),
    ("it", "it-IT", "Italian", "Italiano"),
    ("nl", "nl-NL", "Dutch", "Nederlands"),
    ("pl", "pl-PL", "Polish", "Polski"),
    ("uk", "uk-UA", "Ukrainian", "Українська"),
    ("vi", "vi-VN", "Vietnamese", "Tiếng Việt"),
    ("th", "th-TH", "Thai", "ไทย"),
    ("fa", "fa-IR", "Persian", "فارسی"),
    ("iw", "he-IL", "Hebrew", "עברית"),
    ("ta", "ta-IN", "Tamil", "தமிழ்"),
    ("te", "te-IN", "Telugu", "తెలుగు"),
    ("mr", "mr-IN", "Marathi", "मराठी"),
    ("gu", "gu-IN", "Gujarati", "ગુજરાતી"),
    ("kn", "kn-IN", "Kannada", "ಕನ್ನಡ"),
    ("ml", "ml-IN", "Malayalam", "മലയാളം"),
    ("pa", "pa-guru-IN", "Punjabi", "ਪੰਜਾਬੀ"),
    ("ne", "ne-NP", "Nepali", "नेपाली"),
    ("si", "si-LK", "Sinhala", "සිංහල"),
    ("sw", "sw-KE", "Swahili", "Kiswahili"),
    ("el", "el-GR", "Greek", "Ελληνικά"),
    ("sv", "sv-SE", "Swedish", "Svenska"),
    ("cs", "cs-CZ", "Czech", "Čeština"),
    ("ro", "ro-RO", "Romanian", "Română"),
    ("hu", "hu-HU", "Hungarian", "Magyar"),
];

/// Languages written without spaces between words.
pub const NO_SPACE: &[&str] = &["zh-CN", "ja", "th"];

pub fn find(code: &str) -> &'static (&'static str, &'static str, &'static str, &'static str) {
    LANGS.iter().find(|l| l.0 == code).unwrap_or(&LANGS[0])
}

pub fn stt_code(code: &str) -> &'static str {
    find(code).1
}

pub fn no_space(code: &str) -> bool {
    NO_SPACE.contains(&code)
}

pub fn display_name(code: &str) -> String {
    let l = find(code);
    if l.2 == l.3 {
        l.2.to_string()
    } else {
        format!("{} \u{00B7} {}", l.2, l.3)
    }
}

pub fn short(code: &str) -> String {
    code.split('-').next().unwrap_or(code).to_uppercase()
}

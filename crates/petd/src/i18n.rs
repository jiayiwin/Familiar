//! Chinese / English. One process-wide switch, set from `language:` in
//! config.yaml (`auto` follows the system UI language).
//!
//! In code: `tr!("中文 {x}", "English {x}")` gives a String, `t("中", "en")`
//! a &str. Data files carry `*_en` fields that the loaders pick up.

use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);

pub fn is_en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// "zh" or "en".
pub fn code() -> &'static str {
    if is_en() {
        "en"
    } else {
        "zh"
    }
}

/// Apply a `language:` value. Returns true when the language changed.
pub fn set_from(lang: &str) -> bool {
    let en = match lang.trim().to_ascii_lowercase().as_str() {
        "en" | "english" => true,
        "zh" | "cn" | "chinese" | "中文" => false,
        _ => system_is_en(),
    };
    EN.swap(en, Ordering::Relaxed) != en
}

/// The system UI language is not Chinese.
pub fn system_is_en() -> bool {
    match sys_locale::get_locale() {
        Some(l) => !l.to_ascii_lowercase().starts_with("zh"),
        None => false,
    }
}

pub fn t<'a>(zh: &'a str, en: &'a str) -> &'a str {
    if is_en() {
        en
    } else {
        zh
    }
}

/// `tr!("中文 {x}", "English {x}")`: a formatted String in the current language.
#[macro_export]
macro_rules! tr {
    ($zh:literal, $en:literal $(,)?) => {
        if $crate::i18n::is_en() { format!($en) } else { format!($zh) }
    };
    ($zh:literal, $en:literal, $($arg:tt)+) => {
        if $crate::i18n::is_en() { format!($en, $($arg)+) } else { format!($zh, $($arg)+) }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_in_the_current_language() {
        // Other tests check Chinese replies, so stay on zh here (the switch is global).
        set_from("zh");
        let n = 3;
        assert_eq!(crate::tr!("{n} 颗糖", "{n} sweets"), "3 颗糖");
        assert_eq!(crate::tr!("{} 颗糖", "{} sweets", n + 1), "4 颗糖");
        assert_eq!(t("糖", "sweet"), "糖");
        assert_eq!(code(), "zh");
    }
}

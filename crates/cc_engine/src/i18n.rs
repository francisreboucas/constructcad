//! Localization: a Fluent-compatible subset (`key = value` messages with
//! `{ $var }` placeholders). All languages are embedded at build time and the
//! active one can be switched at runtime.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    PtBr,
    En,
    Es,
    Fr,
    De,
    It,
    Lo,
    Pl,
    Uk,
    ZhTw,
    ZhCn,
    Ja,
}

impl Language {
    pub const ALL: [Language; 12] = [
        Language::PtBr,
        Language::En,
        Language::Es,
        Language::Fr,
        Language::De,
        Language::It,
        Language::Lo,
        Language::Pl,
        Language::Uk,
        Language::ZhTw,
        Language::ZhCn,
        Language::Ja,
    ];

    /// BCP 47 code, used for persistence.
    pub fn code(self) -> &'static str {
        match self {
            Language::PtBr => "pt-BR",
            Language::En => "en",
            Language::Es => "es",
            Language::Fr => "fr",
            Language::De => "de",
            Language::It => "it",
            Language::Lo => "lo",
            Language::Pl => "pl",
            Language::Uk => "uk-UA",
            Language::ZhTw => "zh-TW",
            Language::ZhCn => "zh-CN",
            Language::Ja => "ja",
        }
    }

    /// Name in the language itself, so it is recognizable whatever is active.
    pub fn native_name(self) -> &'static str {
        match self {
            Language::PtBr => "Português (Brasil)",
            Language::En => "English",
            Language::Es => "Español",
            Language::Fr => "Français",
            Language::De => "Deutsch",
            Language::It => "Italiano",
            Language::Lo => "ພາສາລາວ",
            Language::Pl => "Polski",
            Language::Uk => "Українська",
            Language::ZhTw => "繁體中文",
            Language::ZhCn => "简体中文",
            Language::Ja => "日本語",
        }
    }

    pub fn from_code(code: &str) -> Option<Language> {
        let code = code.to_ascii_lowercase();
        // Chinese: traditional forms for Taiwan, Hong Kong, Macau and Hant
        // (zh-TW, zh-HK, zh-Hant…); simplified for the rest (zh, zh-CN,
        // zh-SG, zh-Hans…). An explicit script wins over the region
        // (zh-Hans-HK is simplified, zh-Hant-CN traditional).
        // POSIX locales may carry an encoding or modifier (zh_TW.UTF-8).
        let tag = code.split(['.', '@']).next().unwrap_or("");
        let parts: Vec<&str> = tag.get(2..).unwrap_or("").split(['-', '_']).filter(|p| !p.is_empty()).collect();
        let traditional = if parts.contains(&"hant") {
            true
        } else if parts.contains(&"hans") {
            false
        } else {
            parts.iter().any(|part| matches!(*part, "tw" | "hk" | "mo"))
        };
        let lang = match code.get(..2)? {
            "pt" => Language::PtBr,
            "en" => Language::En,
            "es" => Language::Es,
            "fr" => Language::Fr,
            "de" => Language::De,
            "it" => Language::It,
            "lo" => Language::Lo,
            "pl" => Language::Pl,
            "uk" => Language::Uk,
            "zh" if traditional => Language::ZhTw,
            "zh" => Language::ZhCn,
            "ja" => Language::Ja,
            _ => return None,
        };
        Some(lang)
    }

    /// Language of the operating system, English when unsupported.
    pub fn from_system() -> Language {
        sys_locale::get_locale().and_then(|l| Language::from_code(&l)).unwrap_or(Language::En)
    }

    /// The embedded message file of the language.
    pub fn source(self) -> &'static str {
        match self {
            Language::PtBr => include_str!("../../../assets/i18n/pt-BR.ftl"),
            Language::En => include_str!("../../../assets/i18n/en.ftl"),
            Language::Es => include_str!("../../../assets/i18n/es.ftl"),
            Language::Fr => include_str!("../../../assets/i18n/fr.ftl"),
            Language::De => include_str!("../../../assets/i18n/de.ftl"),
            Language::It => include_str!("../../../assets/i18n/it.ftl"),
            Language::Lo => include_str!("../../../assets/i18n/lo.ftl"),
            Language::Pl => include_str!("../../../assets/i18n/pl.ftl"),
            Language::Uk => include_str!("../../../assets/i18n/uk-UA.ftl"),
            Language::ZhTw => include_str!("../../../assets/i18n/zh-TW.ftl"),
            Language::ZhCn => include_str!("../../../assets/i18n/zh-CN.ftl"),
            Language::Ja => include_str!("../../../assets/i18n/ja.ftl"),
        }
    }

    fn index(self) -> usize {
        Language::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    /// Whether numbers use a decimal comma.
    pub fn decimal_comma(self) -> bool {
        !matches!(self, Language::En | Language::ZhTw | Language::ZhCn | Language::Ja)
    }
}

static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn bundle(lang: Language) -> &'static HashMap<String, String> {
    static BUNDLES: OnceLock<Vec<HashMap<String, String>>> = OnceLock::new();
    &BUNDLES.get_or_init(|| Language::ALL.iter().map(|l| parse_messages(l.source())).collect())[lang.index()]
}

pub fn language() -> Language {
    Language::ALL[CURRENT.load(Ordering::Relaxed)]
}

/// Switch the active language (also sets the number format).
pub fn set_language(lang: Language) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
    cc_views::format::set_decimal_comma(lang.decimal_comma());
}

/// Parse `key = value` lines; `#` starts a comment line.
pub fn parse_messages(source: &str) -> HashMap<String, String> {
    source
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Translate `key` if the active language defines it.
pub fn try_tr(key: &str) -> Option<String> {
    bundle(language()).get(key).cloned()
}

/// Translate `key`; falls back to Portuguese, then to the key itself so gaps are visible.
pub fn tr(key: &str) -> String {
    try_tr(key).or_else(|| bundle(Language::PtBr).get(key).cloned()).unwrap_or_else(|| key.to_string())
}

/// Translate `key`, substituting `{ $name }` placeholders.
pub fn tr_args(key: &str, args: &[(&str, String)]) -> String {
    let mut s = tr(key);
    for (name, value) in args {
        s = s.replace(&format!("{{ ${name} }}"), value);
    }
    s
}

/// `t!("key")` or `t!("key", "name" => value, ...)`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:literal => $value:expr),+ $(,)?) => {
        $crate::i18n::tr_args($key, &[$(($name, $value.to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn placeholders() {
        let m = parse_messages("a = Olá { $name }!\n# comment\nb=x");
        assert_eq!(m["a"], "Olá { $name }!");
        assert_eq!(m["b"], "x");
    }

    #[test]
    fn language_codes() {
        assert_eq!(Language::from_code("pt_BR"), Some(Language::PtBr));
        assert_eq!(Language::from_code("en-US"), Some(Language::En));
        assert_eq!(Language::from_code("es-MX"), Some(Language::Es));
        assert_eq!(Language::from_code("es-ES"), Some(Language::Es));
        assert_eq!(Language::from_code("fr-FR"), Some(Language::Fr));
        assert_eq!(Language::from_code("de_DE"), Some(Language::De));
        assert_eq!(Language::from_code("it-IT"), Some(Language::It));
        assert_eq!(Language::from_code("lo-LA"), Some(Language::Lo));
        assert_eq!(Language::from_code("pl-PL"), Some(Language::Pl));
        assert_eq!(Language::from_code("uk-UA"), Some(Language::Uk));
        assert_eq!(Language::from_code("zh-TW"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("zh_HK"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("zh-Hant-TW"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("zh-MO"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("zh_TW.UTF-8"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("zh-Hant-CN"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("zh-CN"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh_CN.UTF-8"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh-SG"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh-Hans"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh-Hans-CN"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh-Hans-HK"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("ja-JP"), Some(Language::Ja));
        assert_eq!(Language::from_code("ja_JP.UTF-8"), Some(Language::Ja));
        assert_eq!(Language::from_code("ko-KR"), None);
        assert_eq!(Language::from_code("p"), None);
        for l in Language::ALL {
            assert_eq!(Language::from_code(l.code()), Some(l));
        }
    }

    fn placeholders_of(s: &str) -> BTreeSet<&str> {
        s.split("{ $").skip(1).filter_map(|p| p.split(" }").next()).collect()
    }

    /// Every language defines exactly the same keys with the same placeholders.
    #[test]
    fn languages_are_complete() {
        let reference = parse_messages(Language::PtBr.source());
        for lang in Language::ALL {
            let messages = parse_messages(lang.source());
            let missing: Vec<_> = reference.keys().filter(|k| !messages.contains_key(*k)).collect();
            let extra: Vec<_> = messages.keys().filter(|k| !reference.contains_key(*k)).collect();
            assert!(missing.is_empty() && extra.is_empty(), "{lang:?}: missing {missing:?}, extra {extra:?}");
            for (k, v) in &messages {
                assert_eq!(placeholders_of(v), placeholders_of(&reference[k]), "{lang:?}: placeholders of {k}");
            }
        }
    }

    /// The localized template resolves every name it asks for.
    #[test]
    fn template_names_translated() {
        let messages = parse_messages(Language::En.source());
        let asked = std::cell::RefCell::new(Vec::new());
        let project = cc_core::Project::from_template_localized(&|k| {
            asked.borrow_mut().push(k.to_string());
            messages.get(k).cloned()
        });
        let missing: Vec<_> = asked.borrow().iter().filter(|k| !messages.contains_key(*k)).cloned().collect();
        assert!(missing.is_empty(), "missing template names: {missing:?}");
        assert!(project.levels.iter().all(|l| !l.name.is_empty()));
        // Layers must reference materials that exist under their translated names.
        for t in project.types.values() {
            for layer in t.layers() {
                assert!(project.materials.iter().any(|m| m.name == layer.material), "{}", layer.material);
            }
        }
    }
}

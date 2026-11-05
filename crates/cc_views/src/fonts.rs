//! Fallback fonts for the scripts Inter lacks (Chinese, Japanese, Korean,
//! Lao), found among the fonts operating systems usually ship and read once.
//! Nothing is bundled, so a system without them shows missing glyphs as
//! boxes, as before.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A font file and the face to use in it (collections hold several).
pub struct FontFile {
    pub data: Vec<u8>,
    pub index: u32,
}

impl FontFile {
    pub fn font_ref(&self) -> Option<skrifa::FontRef<'_>> {
        skrifa::FontRef::from_index(&self.data, self.index).ok()
    }
}

/// Larger files are not fonts we want to hold in memory.
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

/// One character of each script to cover: a Simplified ideograph ("图")
/// and its Traditional form ("圖"), so both Chinese interfaces find their
/// glyphs even when the first font holds only one set, kana ("あ") and
/// Hangul ("한").
const PROBES: [char; 4] = ['\u{56FE}', '\u{5716}', '\u{3042}', '\u{D55C}'];

/// The system's CJK fonts in fallback order (empty when there is none).
pub fn cjk() -> &'static [FontFile] {
    static CJK: OnceLock<Vec<FontFile>> = OnceLock::new();
    CJK.get_or_init(|| load_covering(&candidates(), &PROBES))
}

/// A Lao letter ("ກ"), to find a font that covers the script.
const LAO_PROBE: char = '\u{0E81}';

/// The system's Lao font (empty when there is none).
pub fn lao() -> &'static [FontFile] {
    static LAO: OnceLock<Vec<FontFile>> = OnceLock::new();
    LAO.get_or_init(|| load_covering(&lao_candidates(), &[LAO_PROBE]))
}

/// Every fallback font, in order: CJK, then Lao.
pub fn fallbacks() -> impl Iterator<Item = &'static FontFile> {
    cjk().iter().chain(lao())
}

/// Readable faces among `candidates`, in order, each kept when it has one
/// of `probes` that no earlier face has; stops once all are covered.
fn load_covering(candidates: &[(PathBuf, u32)], probes: &[char]) -> Vec<FontFile> {
    use skrifa::MetadataProvider;
    let mut missing = probes.to_vec();
    let mut out = Vec::new();
    for (path, index) in candidates {
        if missing.is_empty() {
            break;
        }
        let Some(file) = load(path, *index) else { continue };
        let Some(font) = file.font_ref() else { continue };
        let charmap = font.charmap();
        let before = missing.len();
        missing.retain(|c| charmap.map(*c).is_none());
        if missing.len() < before {
            out.push(file);
        }
    }
    out
}

fn load(path: &Path, index: u32) -> Option<FontFile> {
    if std::fs::metadata(path).ok()?.len() > MAX_FONT_BYTES {
        return None;
    }
    Some(FontFile { data: std::fs::read(path).ok()?, index })
}

/// Usual CJK fonts: Traditional Chinese forms first, then those that add
/// Hangul or kana, then the rest.
fn candidates() -> Vec<(PathBuf, u32)> {
    let mut out = Vec::new();
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let fonts = dir.join("Fonts");
        for name in ["msjh.ttc", "malgun.ttf", "YuGothR.ttc", "meiryo.ttc", "msyh.ttc", "mingliu.ttc", "simsun.ttc"] {
            out.push((fonts.join(name), 0));
        }
    } else if cfg!(target_os = "macos") {
        for name in [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/Library/Fonts/Arial Unicode.ttf",
        ] {
            out.push((PathBuf::from(name), 0));
        }
    } else {
        // Noto Sans CJK collections hold JP, KR, SC, TC, HK in that order.
        for name in [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-VF.ttc",
            "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc",
        ] {
            out.push((PathBuf::from(name), 3));
        }
        for name in [
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ] {
            out.push((PathBuf::from(name), 0));
        }
    }
    out
}

/// Usual fonts with Lao letters.
fn lao_candidates() -> Vec<(PathBuf, u32)> {
    let names: Vec<PathBuf> = if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        // Leelawadee UI (Windows 10 and later), Lao UI (7 and 8), DokChampa (older).
        ["LeelawUI.ttf", "LaoUI.ttf", "DokChamp.ttf"].iter().map(|n| dir.join("Fonts").join(n)).collect()
    } else if cfg!(target_os = "macos") {
        [
            "/System/Library/Fonts/Supplemental/Lao Sangam MN.ttf",
            "/System/Library/Fonts/Supplemental/Lao MN.ttc",
            "/System/Library/Fonts/LaoSangamMN.ttf",
            "/Library/Fonts/Lao Sangam MN.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    } else {
        [
            "/usr/share/fonts/truetype/noto/NotoSansLao-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansLao-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSansLao-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansLaoUI-Regular.ttf",
            "/usr/share/fonts/truetype/lao/Phetsarath_OT.ttf",
            "/usr/share/fonts/truetype/fonts-lao/Phetsarath_OT.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    };
    names.into_iter().map(|p| (p, 0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_readable_faces_that_add_coverage() {
        let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts");
        let inter = fonts.join("Inter-Regular.ttf");
        let candidates = vec![
            (fonts.join("missing.ttf"), 0),
            // Not a font.
            (fonts.join("OFL-Inter.txt"), 0),
            // A face index past the file's faces.
            (inter.clone(), 5),
            (inter.clone(), 0),
            // Covers nothing new.
            (fonts.join("Inter-SemiBold.ttf"), 0),
        ];
        let found = load_covering(&candidates, &['A', 'B']);
        assert_eq!(found.len(), 1);
        assert!(found[0].font_ref().is_some());
        // Inter has no ideographs, kana or Hangul, nor Lao letters.
        assert!(load_covering(&candidates, &PROBES).is_empty());
        assert!(load_covering(&candidates, &[LAO_PROBE]).is_empty());
    }

    /// A system with a Lao font (Windows 10 and later have Leelawadee UI)
    /// gives the Lao letters from it.
    #[test]
    fn lao_letters_come_from_a_system_font() {
        use skrifa::MetadataProvider;
        if let Some(font) = lao().first().and_then(|f| f.font_ref()) {
            assert!(font.charmap().map(LAO_PROBE).is_some());
            assert_eq!(fallbacks().count(), cjk().len() + lao().len());
        }
    }

    /// A system with CJK fonts draws every character of the Chinese
    /// interfaces, Simplified and Traditional, and the Japanese one from them.
    #[test]
    fn chinese_interfaces_come_from_system_fonts() {
        use skrifa::MetadataProvider;
        let fonts: Vec<_> = cjk().iter().filter_map(|f| f.font_ref()).collect();
        if fonts.is_empty() {
            return;
        }
        for (name, source) in [
            ("zh-CN", include_str!("../../../assets/i18n/zh-CN.ftl")),
            ("zh-TW", include_str!("../../../assets/i18n/zh-TW.ftl")),
            ("ja", include_str!("../../../assets/i18n/ja.ftl")),
        ] {
            let missing: std::collections::BTreeSet<char> = source
                .chars()
                .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
                .filter(|c| fonts.iter().all(|f| f.charmap().map(*c).is_none()))
                .collect();
            assert!(missing.is_empty(), "{name}: no system font has {missing:?}");
        }
    }
}

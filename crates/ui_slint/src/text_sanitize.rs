//! Normalizes provider-supplied display text so stylized Unicode characters never draw as boxes.
//!
//! Several CloudStream plugins fake bold/italic formatting in plain-text plots (such as IMDb award
//! prefixes like `[𝟭 𝘄𝗶𝗻 & 𝟳 𝗻𝗼𝗺𝗶𝗻𝗮𝘁𝗶𝗼𝗻𝘀 𝘁𝗼𝘁𝗮𝗹]`) using Unicode Mathematical Alphanumeric
//! Symbols (`U+1D400..=U+1D7FF`). Those characters live in `Script::Common`, lack glyphs in
//! Windows' default `Segoe UI` / `Arial` stack, and render as missing-glyph squares unless folded
//! back to standard ASCII letters and digits.

/// Start of the 13 consecutive 52-letter Latin styles in Mathematical Alphanumeric Symbols.
const MATH_LATIN_START: u32 = 0x1D400;
/// End of the 13 Latin styles (Monospace `z`).
const MATH_LATIN_END: u32 = 0x1D6A3;
/// Start of the 5 consecutive 10-digit styles in Mathematical Alphanumeric Symbols.
const MATH_DIGIT_START: u32 = 0x1D7CE;
/// End of the 5 digit styles (Monospace `9`).
const MATH_DIGIT_END: u32 = 0x1D7FF;
/// Number of uppercase + lowercase Latin letters per mathematical alphabet style.
const LATIN_STYLE_SPAN: u32 = 52;
/// Number of uppercase Latin letters in one alphabet.
const LATIN_CASE_SPAN: u32 = 26;
/// Number of decimal digits per mathematical digit style.
const DIGIT_STYLE_SPAN: u32 = 10;

/// Returns `input` with Unicode mathematical alphanumerics folded to ASCII and invisible
/// non-rendering characters cleaned, preserving newlines and all non-Latin scripts.
pub(crate) fn clean_display_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
            continue;
        }
        if let Some(normalized) = normalize_char(ch) {
            out.push(normalized);
        }
    }
    out
}

fn normalize_char(ch: char) -> Option<char> {
    match ch {
        // Invisible / non-rendering format characters that can draw as missing-glyph boxes.
        '\u{00AD}' | '\u{200B}' | '\u{FEFF}' => None,
        // Unicode line and paragraph separators.
        '\u{2028}' | '\u{2029}' => Some('\n'),
        _ => Some(
            fold_math_alphanumeric(ch)
                .or_else(|| fold_math_letterlike(ch))
                .unwrap_or(ch),
        ),
    }
}

fn fold_math_alphanumeric(ch: char) -> Option<char> {
    let code = u32::from(ch);
    if (MATH_LATIN_START..=MATH_LATIN_END).contains(&code) {
        let offset = (code - MATH_LATIN_START) % LATIN_STYLE_SPAN;
        let base = if offset < LATIN_CASE_SPAN { b'A' } else { b'a' };
        let index = u8::try_from(offset % LATIN_CASE_SPAN).ok()?;
        return Some(char::from(base + index));
    }
    if (MATH_DIGIT_START..=MATH_DIGIT_END).contains(&code) {
        let digit = u8::try_from((code - MATH_DIGIT_START) % DIGIT_STYLE_SPAN).ok()?;
        return Some(char::from(b'0' + digit));
    }
    None
}

/// Maps the 24 Letterlike Symbol codepoints that fill reserved holes in the Mathematical
/// Alphanumeric Symbols block (Italic `h`, Script, Fraktur, and Double-Struck letters).
fn fold_math_letterlike(ch: char) -> Option<char> {
    let ascii = match ch {
        '\u{2102}' | '\u{212D}' => 'C',
        '\u{210A}' => 'g',
        '\u{210B}' | '\u{210C}' | '\u{210D}' => 'H',
        '\u{210E}' => 'h',
        '\u{2110}' | '\u{2111}' => 'I',
        '\u{2112}' => 'L',
        '\u{2115}' => 'N',
        '\u{2119}' => 'P',
        '\u{211A}' => 'Q',
        '\u{211B}' | '\u{211C}' | '\u{211D}' => 'R',
        '\u{2124}' | '\u{2128}' => 'Z',
        '\u{212C}' => 'B',
        '\u{212F}' => 'e',
        '\u{2130}' => 'E',
        '\u{2131}' => 'F',
        '\u{2133}' => 'M',
        '\u{2134}' => 'o',
        _ => return None,
    };
    Some(ascii)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_display_text_folds_recorded_provider_awards_prefixes() {
        let movie_plot = "[𝟭 𝗻𝗼𝗺𝗶𝗻𝗮𝘁𝗶𝗼𝗻 𝘁𝗼𝘁𝗮𝗹] A forgotten Peter Parker lives alone.";
        assert_eq!(
            clean_display_text(movie_plot),
            "[1 nomination total] A forgotten Peter Parker lives alone."
        );

        let series_plot = "[𝟭 𝘄𝗶𝗻 & 𝟳 𝗻𝗼𝗺𝗶𝗻𝗮𝘁𝗶𝗼𝗻𝘀 𝘁𝗼𝘁𝗮𝗹] Maggie and Negan travel into Manhattan.";
        assert_eq!(
            clean_display_text(series_plot),
            "[1 win & 7 nominations total] Maggie and Negan travel into Manhattan."
        );
    }

    #[test]
    fn clean_display_text_folds_all_math_styles_and_letterlike_holes() {
        assert_eq!(
            clean_display_text("𝐀𝐳 𝐴𝑧 𝑨𝒛 𝒜𝓏 𝓐𝔃 𝔄𝔷 𝔸𝕫"),
            "Az Az Az Az Az Az Az"
        );
        assert_eq!(
            clean_display_text("𝕬𝖟 𝖠𝗓 𝗔𝗭𝗮𝘇 𝘈𝘻 𝘼𝙯 𝙰𝚣"),
            "Az Az AZaz Az Az Az"
        );
        assert_eq!(clean_display_text("𝟎𝟗 𝟘𝟡 𝟢𝟫 𝟬𝟵 𝟶𝟿"), "09 09 09 09 09");
        assert_eq!(
            clean_display_text("ℂℍℕℙℚℝℤ ℎ ℬℰℱℋℐℒℳℛ ℯℊℴ ℭℌℑℜℨ"),
            "CHNPQRZ h BEFHILMR ego CHIRZ"
        );
    }

    #[test]
    fn clean_display_text_strips_invisible_chars_and_preserves_scripts_and_newlines() {
        let mixed = "\u{FEFF}Line\u{00AD}1\r\nLine\u{200B}2\u{2028}مرحبا 日本語 Кириллица — “ok”";
        assert_eq!(
            clean_display_text(mixed),
            "Line1\nLine2\nمرحبا 日本語 Кириллица — “ok”"
        );
    }
}

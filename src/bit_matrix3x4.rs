//! Internal 3×4 pixel font for LED matrix displays.

type GlyphRows = [u8; 4];

/// Look up the 3×4 rows for an ASCII character.
pub(crate) fn glyph_rows(character: char) -> GlyphRows {
    let code_point = character as u32;
    if code_point <= 0x7F {
        glyph_for(code_point as u8)
    } else {
        Glyphs::BLANK
    }
}

const fn glyph_for(byte: u8) -> GlyphRows {
    match byte {
        b' ' => Glyphs::BLANK,
        b'!' => Glyphs::EXCLAMATION,
        b'"' => Glyphs::DOUBLE_QUOTE,
        b'#' => Glyphs::HASH,
        b'$' => Glyphs::DOLLAR,
        b'%' => Glyphs::PERCENT,
        b'&' => Glyphs::AMPERSAND,
        b'\'' => Glyphs::APOSTROPHE,
        b'(' => Glyphs::LEFT_PAREN,
        b')' => Glyphs::RIGHT_PAREN,
        b'*' => Glyphs::ASTERISK,
        b'+' => Glyphs::PLUS,
        b',' => Glyphs::COMMA,
        b'-' => Glyphs::DASH,
        b'.' => Glyphs::DOT,
        b'/' => Glyphs::SLASH,
        b'0' => Glyphs::DIGIT_0,
        b'1' => Glyphs::DIGIT_1,
        b'2' => Glyphs::DIGIT_2,
        b'3' => Glyphs::DIGIT_3,
        b'4' => Glyphs::DIGIT_4,
        b'5' => Glyphs::DIGIT_5,
        b'6' => Glyphs::DIGIT_6,
        b'7' => Glyphs::DIGIT_7,
        b'8' => Glyphs::DIGIT_8,
        b'9' => Glyphs::DIGIT_9,
        b':' => Glyphs::COLON,
        b';' => Glyphs::SEMICOLON,
        b'<' => Glyphs::LESS_THAN,
        b'=' => Glyphs::EQUALS,
        b'>' => Glyphs::GREATER_THAN,
        b'?' => Glyphs::QUESTION,
        b'@' => Glyphs::AT,
        b'A' | b'a' => Glyphs::A,
        b'B' | b'b' => Glyphs::B,
        b'C' | b'c' => Glyphs::C,
        b'D' | b'd' => Glyphs::D,
        b'E' | b'e' => Glyphs::E,
        b'F' | b'f' => Glyphs::F,
        b'G' | b'g' => Glyphs::G,
        b'H' | b'h' => Glyphs::H,
        b'I' | b'i' => Glyphs::I,
        b'J' | b'j' => Glyphs::J,
        b'K' | b'k' => Glyphs::K,
        b'L' | b'l' => Glyphs::L,
        b'M' | b'm' => Glyphs::M,
        b'N' | b'n' => Glyphs::N,
        b'O' | b'o' => Glyphs::O,
        b'P' | b'p' => Glyphs::P,
        b'Q' | b'q' => Glyphs::Q,
        b'R' | b'r' => Glyphs::R,
        b'S' | b's' => Glyphs::S,
        b'T' | b't' => Glyphs::T,
        b'U' | b'u' => Glyphs::U,
        b'V' | b'v' => Glyphs::V,
        b'W' | b'w' => Glyphs::W,
        b'X' | b'x' => Glyphs::X,
        b'Y' | b'y' => Glyphs::Y,
        b'Z' | b'z' => Glyphs::Z,
        b'[' => Glyphs::LEFT_BRACKET,
        b'\\' => Glyphs::BACKSLASH,
        b']' => Glyphs::RIGHT_BRACKET,
        b'^' => Glyphs::CARET,
        b'_' => Glyphs::UNDERSCORE,
        b'`' => Glyphs::GRAVE,
        b'{' => Glyphs::LEFT_BRACE,
        b'|' => Glyphs::BAR,
        b'}' => Glyphs::RIGHT_BRACE,
        b'~' => Glyphs::TILDE,
        _ => Glyphs::BLANK,
    }
}

struct Glyphs;

impl Glyphs {
    const BLANK: GlyphRows = [0; 4];

    const EXCLAMATION: GlyphRows = [0b010, 0b010, 0b010, 0b010];
    const DOUBLE_QUOTE: GlyphRows = [0b101, 0b101, 0b000, 0b000];
    const HASH: GlyphRows = [0b101, 0b111, 0b111, 0b101];
    const DOLLAR: GlyphRows = [0b010, 0b111, 0b010, 0b111];
    const PERCENT: GlyphRows = [0b100, 0b001, 0b010, 0b001];
    const AMPERSAND: GlyphRows = [0b010, 0b101, 0b010, 0b101];
    const APOSTROPHE: GlyphRows = [0b010, 0b010, 0b000, 0b000];
    const LEFT_PAREN: GlyphRows = [0b010, 0b100, 0b100, 0b010];
    const RIGHT_PAREN: GlyphRows = [0b010, 0b001, 0b001, 0b010];
    const ASTERISK: GlyphRows = [0b101, 0b010, 0b101, 0b000];
    const PLUS: GlyphRows = [0b010, 0b111, 0b010, 0b000];
    const COMMA: GlyphRows = [0b000, 0b000, 0b010, 0b100];
    const DASH: GlyphRows = [0b000, 0b000, 0b111, 0b000];
    const DOT: GlyphRows = [0b000, 0b000, 0b000, 0b010];
    const SLASH: GlyphRows = [0b001, 0b010, 0b100, 0b000];

    const DIGIT_0: GlyphRows = [0b111, 0b101, 0b101, 0b111];
    const DIGIT_1: GlyphRows = [0b010, 0b110, 0b010, 0b111];
    const DIGIT_2: GlyphRows = [0b110, 0b001, 0b010, 0b111];
    const DIGIT_3: GlyphRows = [0b111, 0b001, 0b011, 0b111];
    const DIGIT_4: GlyphRows = [0b101, 0b101, 0b111, 0b001];
    const DIGIT_5: GlyphRows = [0b111, 0b100, 0b011, 0b111];
    const DIGIT_6: GlyphRows = [0b100, 0b111, 0b101, 0b111];
    const DIGIT_7: GlyphRows = [0b111, 0b001, 0b010, 0b100];
    const DIGIT_8: GlyphRows = [0b111, 0b101, 0b010, 0b111];
    const DIGIT_9: GlyphRows = [0b111, 0b101, 0b111, 0b001];

    const COLON: GlyphRows = [0b010, 0b000, 0b010, 0b000];
    const SEMICOLON: GlyphRows = [0b010, 0b000, 0b010, 0b100];
    const LESS_THAN: GlyphRows = [0b001, 0b010, 0b100, 0b010];
    const EQUALS: GlyphRows = [0b000, 0b111, 0b000, 0b111];
    const GREATER_THAN: GlyphRows = [0b100, 0b010, 0b001, 0b010];
    const QUESTION: GlyphRows = [0b111, 0b001, 0b010, 0b010];
    const AT: GlyphRows = [0b111, 0b101, 0b111, 0b100];

    const A: GlyphRows = [0b111, 0b101, 0b111, 0b101];
    const B: GlyphRows = [0b110, 0b111, 0b101, 0b110];
    const C: GlyphRows = [0b111, 0b100, 0b100, 0b111];
    const D: GlyphRows = [0b110, 0b101, 0b101, 0b110];
    const E: GlyphRows = [0b111, 0b110, 0b100, 0b111];
    const F: GlyphRows = [0b111, 0b110, 0b100, 0b100];
    const G: GlyphRows = [0b111, 0b100, 0b101, 0b111];
    const H: GlyphRows = [0b101, 0b111, 0b101, 0b101];
    const I: GlyphRows = [0b111, 0b010, 0b010, 0b111];
    const J: GlyphRows = [0b001, 0b001, 0b101, 0b111];
    const K: GlyphRows = [0b101, 0b110, 0b101, 0b101];
    const L: GlyphRows = [0b100, 0b100, 0b100, 0b111];
    const M: GlyphRows = [0b111, 0b111, 0b101, 0b101];
    const N: GlyphRows = [0b111, 0b111, 0b111, 0b101];
    const O: GlyphRows = [0b111, 0b101, 0b101, 0b111];
    const P: GlyphRows = [0b110, 0b111, 0b110, 0b100];
    const Q: GlyphRows = [0b111, 0b101, 0b111, 0b001];
    const R: GlyphRows = [0b110, 0b111, 0b110, 0b101];
    const S: GlyphRows = [0b111, 0b110, 0b011, 0b111];
    const T: GlyphRows = [0b111, 0b010, 0b010, 0b010];
    const U: GlyphRows = [0b101, 0b101, 0b101, 0b111];
    const V: GlyphRows = [0b101, 0b101, 0b101, 0b010];
    const W: GlyphRows = [0b101, 0b101, 0b111, 0b111];
    const X: GlyphRows = [0b101, 0b010, 0b010, 0b101];
    const Y: GlyphRows = [0b101, 0b010, 0b010, 0b010];
    const Z: GlyphRows = [0b111, 0b001, 0b010, 0b111];

    const LEFT_BRACKET: GlyphRows = [0b110, 0b100, 0b100, 0b110];
    const BACKSLASH: GlyphRows = [0b100, 0b010, 0b001, 0b000];
    const RIGHT_BRACKET: GlyphRows = [0b011, 0b001, 0b001, 0b011];
    const CARET: GlyphRows = [0b010, 0b101, 0b000, 0b000];
    const UNDERSCORE: GlyphRows = [0b000, 0b000, 0b000, 0b111];
    const GRAVE: GlyphRows = [0b010, 0b001, 0b000, 0b000];
    const LEFT_BRACE: GlyphRows = [0b011, 0b010, 0b010, 0b011];
    const BAR: GlyphRows = [0b010, 0b010, 0b010, 0b010];
    const RIGHT_BRACE: GlyphRows = [0b110, 0b010, 0b010, 0b110];
    const TILDE: GlyphRows = [0b000, 0b101, 0b010, 0b000];
}

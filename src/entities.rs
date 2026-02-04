//! HTML entity decoding with SIMD-accelerated ampersand finding.

use crate::simd;

/// Decode HTML entities in a string.
///
/// Supports:
/// - Named entities: `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&nbsp;`, etc.
/// - Decimal numeric entities: `&#123;`
/// - Hexadecimal numeric entities: `&#x7B;`, `&#X7b;`
pub fn decode_entities(input: &str) -> String {
    let bytes = input.as_bytes();

    // Quick check: if no ampersand, return as-is
    if simd::find_amp(bytes).is_none() {
        return input.to_string();
    }

    let mut result = String::with_capacity(input.len());
    let mut pos = 0;

    while pos < bytes.len() {
        // Use SIMD to find the next ampersand
        if let Some(amp_offset) = simd::find_amp(&bytes[pos..]) {
            // Copy text before the ampersand
            result.push_str(&input[pos..pos + amp_offset]);
            pos += amp_offset;

            // Try to decode the entity
            if let Some((decoded, len)) = decode_entity(&input[pos..]) {
                result.push_str(decoded);
                pos += len;
            } else {
                // Not a valid entity, copy the ampersand literally
                result.push('&');
                pos += 1;
            }
        } else {
            // No more ampersands, copy the rest
            result.push_str(&input[pos..]);
            break;
        }
    }

    result
}

/// Try to decode an entity at the start of the input.
/// Returns `Some((decoded_string, consumed_length))` if successful.
fn decode_entity(input: &str) -> Option<(&'static str, usize)> {
    if !input.starts_with('&') {
        return None;
    }

    let input = &input[1..]; // Skip the '&'

    // Find the semicolon
    let semi_pos = input.find(';')?;

    // Don't allow extremely long entities
    if semi_pos > 32 {
        return None;
    }

    let entity = &input[..semi_pos];
    let total_len = semi_pos + 2; // +1 for '&', +1 for ';'

    // Numeric entity
    if entity.starts_with('#') {
        let num_str = &entity[1..];
        let (radix, num_str) = if num_str.starts_with('x') || num_str.starts_with('X') {
            (16, &num_str[1..])
        } else {
            (10, num_str)
        };

        if let Ok(code_point) = u32::from_str_radix(num_str, radix) {
            if let Some(c) = char::from_u32(code_point) {
                // Return a static string for common characters
                return Some((char_to_static_str(c), total_len));
            }
        }
        return None;
    }

    // Named entity
    decode_named_entity(entity).map(|decoded| (decoded, total_len))
}

/// Decode a named HTML entity.
fn decode_named_entity(name: &str) -> Option<&'static str> {
    // Common entities - using a match for fast lookup
    match name {
        // Most common
        "amp" => Some("&"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        "nbsp" => Some("\u{00A0}"),

        // Latin characters
        "Agrave" => Some("À"),
        "Aacute" => Some("Á"),
        "Acirc" => Some("Â"),
        "Atilde" => Some("Ã"),
        "Auml" => Some("Ä"),
        "Aring" => Some("Å"),
        "AElig" => Some("Æ"),
        "Ccedil" => Some("Ç"),
        "Egrave" => Some("È"),
        "Eacute" => Some("É"),
        "Ecirc" => Some("Ê"),
        "Euml" => Some("Ë"),
        "Igrave" => Some("Ì"),
        "Iacute" => Some("Í"),
        "Icirc" => Some("Î"),
        "Iuml" => Some("Ï"),
        "ETH" => Some("Ð"),
        "Ntilde" => Some("Ñ"),
        "Ograve" => Some("Ò"),
        "Oacute" => Some("Ó"),
        "Ocirc" => Some("Ô"),
        "Otilde" => Some("Õ"),
        "Ouml" => Some("Ö"),
        "Oslash" => Some("Ø"),
        "Ugrave" => Some("Ù"),
        "Uacute" => Some("Ú"),
        "Ucirc" => Some("Û"),
        "Uuml" => Some("Ü"),
        "Yacute" => Some("Ý"),
        "THORN" => Some("Þ"),
        "szlig" => Some("ß"),
        "agrave" => Some("à"),
        "aacute" => Some("á"),
        "acirc" => Some("â"),
        "atilde" => Some("ã"),
        "auml" => Some("ä"),
        "aring" => Some("å"),
        "aelig" => Some("æ"),
        "ccedil" => Some("ç"),
        "egrave" => Some("è"),
        "eacute" => Some("é"),
        "ecirc" => Some("ê"),
        "euml" => Some("ë"),
        "igrave" => Some("ì"),
        "iacute" => Some("í"),
        "icirc" => Some("î"),
        "iuml" => Some("ï"),
        "eth" => Some("ð"),
        "ntilde" => Some("ñ"),
        "ograve" => Some("ò"),
        "oacute" => Some("ó"),
        "ocirc" => Some("ô"),
        "otilde" => Some("õ"),
        "ouml" => Some("ö"),
        "oslash" => Some("ø"),
        "ugrave" => Some("ù"),
        "uacute" => Some("ú"),
        "ucirc" => Some("û"),
        "uuml" => Some("ü"),
        "yacute" => Some("ý"),
        "thorn" => Some("þ"),
        "yuml" => Some("ÿ"),

        // Punctuation and symbols
        "copy" => Some("©"),
        "reg" => Some("®"),
        "trade" => Some("™"),
        "times" => Some("×"),
        "divide" => Some("÷"),
        "plusmn" => Some("±"),
        "deg" => Some("°"),
        "micro" => Some("µ"),
        "para" => Some("¶"),
        "sect" => Some("§"),
        "cent" => Some("¢"),
        "pound" => Some("£"),
        "yen" => Some("¥"),
        "euro" => Some("€"),
        "curren" => Some("¤"),

        // Quotation marks
        "ldquo" => Some("\u{201C}"),  // "
        "rdquo" => Some("\u{201D}"),  // "
        "lsquo" => Some("\u{2018}"),  // '
        "rsquo" => Some("\u{2019}"),  // '
        "bdquo" => Some("\u{201E}"),  // „
        "sbquo" => Some("\u{201A}"),  // ‚
        "laquo" => Some("\u{00AB}"),  // «
        "raquo" => Some("\u{00BB}"),  // »

        // Dashes and spaces
        "mdash" => Some("—"),
        "ndash" => Some("–"),
        "ensp" => Some("\u{2002}"),
        "emsp" => Some("\u{2003}"),
        "thinsp" => Some("\u{2009}"),

        // Arrows
        "larr" => Some("←"),
        "rarr" => Some("→"),
        "uarr" => Some("↑"),
        "darr" => Some("↓"),
        "harr" => Some("↔"),

        // Mathematical
        "minus" => Some("−"),
        "lowast" => Some("∗"),
        "radic" => Some("√"),
        "infin" => Some("∞"),
        "ne" => Some("≠"),
        "le" => Some("≤"),
        "ge" => Some("≥"),
        "equiv" => Some("≡"),
        "sum" => Some("∑"),
        "prod" => Some("∏"),
        "int" => Some("∫"),
        "part" => Some("∂"),
        "nabla" => Some("∇"),
        "forall" => Some("∀"),
        "exist" => Some("∃"),
        "empty" => Some("∅"),
        "isin" => Some("∈"),
        "notin" => Some("∉"),
        "sub" => Some("⊂"),
        "sup" => Some("⊃"),
        "sube" => Some("⊆"),
        "supe" => Some("⊇"),
        "and" => Some("∧"),
        "or" => Some("∨"),
        "cap" => Some("∩"),
        "cup" => Some("∪"),

        // Greek letters (common ones)
        "alpha" => Some("α"),
        "beta" => Some("β"),
        "gamma" => Some("γ"),
        "delta" => Some("δ"),
        "epsilon" => Some("ε"),
        "zeta" => Some("ζ"),
        "eta" => Some("η"),
        "theta" => Some("θ"),
        "iota" => Some("ι"),
        "kappa" => Some("κ"),
        "lambda" => Some("λ"),
        "mu" => Some("μ"),
        "nu" => Some("ν"),
        "xi" => Some("ξ"),
        "omicron" => Some("ο"),
        "pi" => Some("π"),
        "rho" => Some("ρ"),
        "sigma" => Some("σ"),
        "tau" => Some("τ"),
        "upsilon" => Some("υ"),
        "phi" => Some("φ"),
        "chi" => Some("χ"),
        "psi" => Some("ψ"),
        "omega" => Some("ω"),
        "Alpha" => Some("Α"),
        "Beta" => Some("Β"),
        "Gamma" => Some("Γ"),
        "Delta" => Some("Δ"),
        "Epsilon" => Some("Ε"),
        "Zeta" => Some("Ζ"),
        "Eta" => Some("Η"),
        "Theta" => Some("Θ"),
        "Iota" => Some("Ι"),
        "Kappa" => Some("Κ"),
        "Lambda" => Some("Λ"),
        "Mu" => Some("Μ"),
        "Nu" => Some("Ν"),
        "Xi" => Some("Ξ"),
        "Omicron" => Some("Ο"),
        "Pi" => Some("Π"),
        "Rho" => Some("Ρ"),
        "Sigma" => Some("Σ"),
        "Tau" => Some("Τ"),
        "Upsilon" => Some("Υ"),
        "Phi" => Some("Φ"),
        "Chi" => Some("Χ"),
        "Psi" => Some("Ψ"),
        "Omega" => Some("Ω"),

        // Other common
        "bull" => Some("•"),
        "hellip" => Some("…"),
        "prime" => Some("′"),
        "Prime" => Some("″"),
        "loz" => Some("◊"),
        "spades" => Some("♠"),
        "clubs" => Some("♣"),
        "hearts" => Some("♥"),
        "diams" => Some("♦"),
        "dagger" => Some("†"),
        "Dagger" => Some("‡"),
        "permil" => Some("‰"),
        "lsaquo" => Some("‹"),
        "rsaquo" => Some("›"),
        "oline" => Some("‾"),
        "frasl" => Some("⁄"),
        "weierp" => Some("℘"),
        "image" => Some("ℑ"),
        "real" => Some("ℜ"),
        "alefsym" => Some("ℵ"),
        "crarr" => Some("↵"),
        "lArr" => Some("⇐"),
        "uArr" => Some("⇑"),
        "rArr" => Some("⇒"),
        "dArr" => Some("⇓"),
        "hArr" => Some("⇔"),

        // Fractions
        "frac12" => Some("½"),
        "frac14" => Some("¼"),
        "frac34" => Some("¾"),
        "frac13" => Some("⅓"),
        "frac23" => Some("⅔"),
        "frac15" => Some("⅕"),
        "frac25" => Some("⅖"),
        "frac35" => Some("⅗"),
        "frac45" => Some("⅘"),
        "frac16" => Some("⅙"),
        "frac56" => Some("⅚"),
        "frac18" => Some("⅛"),
        "frac38" => Some("⅜"),
        "frac58" => Some("⅝"),
        "frac78" => Some("⅞"),

        _ => None,
    }
}

/// Convert a char to a static string.
/// For common characters, returns a &'static str.
/// For others, we need to leak memory (or use a different approach).
fn char_to_static_str(c: char) -> &'static str {
    // For common ASCII and Latin-1 characters, use static strings
    match c {
        '\0' => "\0",
        '\t' => "\t",
        '\n' => "\n",
        '\r' => "\r",
        ' ' => " ",
        '!' => "!",
        '"' => "\"",
        '#' => "#",
        '$' => "$",
        '%' => "%",
        '&' => "&",
        '\'' => "'",
        '(' => "(",
        ')' => ")",
        '*' => "*",
        '+' => "+",
        ',' => ",",
        '-' => "-",
        '.' => ".",
        '/' => "/",
        '0' => "0",
        '1' => "1",
        '2' => "2",
        '3' => "3",
        '4' => "4",
        '5' => "5",
        '6' => "6",
        '7' => "7",
        '8' => "8",
        '9' => "9",
        ':' => ":",
        ';' => ";",
        '<' => "<",
        '=' => "=",
        '>' => ">",
        '?' => "?",
        '@' => "@",
        'A'..='Z' => {
            const UPPER: [&str; 26] = [
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ];
            UPPER[(c as u8 - b'A') as usize]
        }
        '[' => "[",
        '\\' => "\\",
        ']' => "]",
        '^' => "^",
        '_' => "_",
        '`' => "`",
        'a'..='z' => {
            const LOWER: [&str; 26] = [
                "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p",
                "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
            ];
            LOWER[(c as u8 - b'a') as usize]
        }
        '{' => "{",
        '|' => "|",
        '}' => "}",
        '~' => "~",
        '\u{00A0}' => "\u{00A0}", // nbsp
        _ => {
            // For other characters, we need to leak memory
            // This is acceptable for rarely-used numeric entities
            Box::leak(c.to_string().into_boxed_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_entities() {
        assert_eq!(decode_entities("Hello world"), "Hello world");
    }

    #[test]
    fn test_common_entities() {
        assert_eq!(decode_entities("&lt;"), "<");
        assert_eq!(decode_entities("&gt;"), ">");
        assert_eq!(decode_entities("&amp;"), "&");
        assert_eq!(decode_entities("&quot;"), "\"");
        assert_eq!(decode_entities("&apos;"), "'");
    }

    #[test]
    fn test_mixed_content() {
        assert_eq!(
            decode_entities("&lt;div&gt;Hello &amp; world&lt;/div&gt;"),
            "<div>Hello & world</div>"
        );
    }

    #[test]
    fn test_numeric_decimal() {
        assert_eq!(decode_entities("&#60;"), "<");
        assert_eq!(decode_entities("&#62;"), ">");
        assert_eq!(decode_entities("&#38;"), "&");
        assert_eq!(decode_entities("&#65;"), "A");
    }

    #[test]
    fn test_numeric_hex() {
        assert_eq!(decode_entities("&#x3C;"), "<");
        assert_eq!(decode_entities("&#x3E;"), ">");
        assert_eq!(decode_entities("&#x26;"), "&");
        assert_eq!(decode_entities("&#X41;"), "A");
    }

    #[test]
    fn test_invalid_entity() {
        // Invalid entities should be preserved
        assert_eq!(decode_entities("&invalid;"), "&invalid;");
        assert_eq!(decode_entities("&;"), "&;");
    }

    #[test]
    fn test_ampersand_without_semicolon() {
        assert_eq!(decode_entities("foo & bar"), "foo & bar");
    }

    #[test]
    fn test_nbsp() {
        assert_eq!(decode_entities("&nbsp;"), "\u{00A0}");
    }

    #[test]
    fn test_copyright() {
        assert_eq!(decode_entities("&copy;"), "©");
    }

    #[test]
    fn test_multiple_entities() {
        assert_eq!(
            decode_entities("&lt;a href=&quot;url&quot;&gt;"),
            "<a href=\"url\">"
        );
    }
}

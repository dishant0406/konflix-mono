//! Magnet links and release sizes.
//!
//! A 20-byte info hash is written as 40 hex characters or 32 base32 characters, and
//! indexers use both. Everything is normalised to lowercase hex, so the torrent map
//! key, the stream and the eviction lookups all agree on one spelling.

const BASE32: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

fn base32_to_hex(value: &str) -> Option<String> {
    let mut buffer: u32 = 0;
    let mut bits = 0;
    let mut bytes = Vec::new();
    for character in value.to_ascii_lowercase().bytes() {
        let index = BASE32.iter().position(|&b| b == character)? as u32;
        buffer = (buffer << 5) | index;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    (bytes.len() == 20).then(|| bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn to_hex_info_hash(value: &str) -> Option<String> {
    let hash = value.to_ascii_lowercase();
    if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(hash);
    }
    if hash.len() == 32 && hash.bytes().all(|b| BASE32.contains(&b)) {
        return base32_to_hex(&hash);
    }
    None
}

pub fn info_hash_from_magnet(magnet: &str) -> Option<String> {
    let start = magnet.find("xt=urn:btih:")? + "xt=urn:btih:".len();
    let hash: String = magnet[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    to_hex_info_hash(&hash)
}

/// The value of a magnet query parameter, undecoded.
fn magnet_param<'a>(magnet: &'a str, name: &str) -> Option<&'a str> {
    let query = magnet.split_once('?')?.1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
}

/// `decodeURIComponent`. `None` for malformed input.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The release name a magnet carries (`dn`).
pub fn display_name(magnet: &str) -> Option<String> {
    percent_decode(magnet_param(magnet, "dn")?)
}

/// The exact size in bytes a magnet carries (`xl`).
pub fn size_from_magnet(magnet: &str) -> Option<u64> {
    let size = magnet_param(magnet, "xl")?;
    size.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| size.parse().ok())?
}

/// Builds a magnet. The `xt` value stays literal: only the display name and the
/// trackers are percent-encoded, because many clients do not expect `urn:btih:` encoded.
pub fn build_magnet(info_hash: &str, name: Option<&str>, trackers: &[String]) -> String {
    let mut parts = vec![format!("xt=urn:btih:{info_hash}")];
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        parts.push(format!("dn={}", encode(name)));
    }
    for tracker in trackers {
        parts.push(format!("tr={}", encode(tracker)));
    }
    format!("magnet:?{}", parts.join("&"))
}

/// Percent-encodes like JavaScript's `encodeURIComponent`.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Turns "1.309 GB" or "700 MiB" into a byte count.
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let value: f64 = text[..split].parse().ok()?;
    let unit = match text[split..].trim().to_ascii_lowercase().as_str() {
        "b" => 1u64,
        "kb" | "kib" => 1 << 10,
        "mb" | "mib" => 1 << 20,
        "gb" | "gib" => 1 << 30,
        "tb" | "tib" => 1 << 40,
        _ => return None,
    };
    Some((value * unit as f64).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_hashes_are_lowercased() {
        let hash = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        assert_eq!(to_hex_info_hash(hash).unwrap(), hash.to_lowercase());
    }

    #[test]
    fn base32_hashes_become_hex() {
        // 20 zero bytes encode to 32 'a' characters.
        assert_eq!(to_hex_info_hash(&"a".repeat(32)).unwrap(), "0".repeat(40));
        assert!(to_hex_info_hash("short").is_none());
        assert!(to_hex_info_hash(&"1".repeat(32)).is_none());
    }

    #[test]
    fn reads_the_hash_out_of_a_magnet() {
        let magnet = format!("magnet:?xt=urn:btih:{}&dn=x", "AB".repeat(20));
        assert_eq!(info_hash_from_magnet(&magnet).unwrap(), "ab".repeat(20));
        assert!(info_hash_from_magnet("magnet:?dn=x").is_none());
    }

    #[test]
    fn builds_a_magnet_with_a_literal_xt() {
        let magnet = build_magnet(
            &"a".repeat(40),
            Some("A B"),
            &["udp://t:1/announce".to_string()],
        );
        assert_eq!(
            magnet,
            format!(
                "magnet:?xt=urn:btih:{}&dn=A%20B&tr=udp%3A%2F%2Ft%3A1%2Fannounce",
                "a".repeat(40)
            )
        );
    }

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("1.5 GB"), Some(1_610_612_736));
        assert_eq!(parse_size("700 MiB"), Some(734_003_200));
        assert_eq!(parse_size("nope"), None);
        assert_eq!(parse_size("12 parsecs"), None);
    }
}

use crate::SongMetadata;
// --- Shared helpers ---
pub fn trim_id3v1_text(b: &[u8]) -> Option<String> {
    let binding = String::from_utf8_lossy(b);
    let s = binding.trim_end_matches('\u{0}').trim();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

pub fn synchsafe_to_u32(bytes: &[u8]) -> u32 {
    ((bytes[0] as u32 & 0x7F) << 21)
        | ((bytes[1] as u32 & 0x7F) << 14)
        | ((bytes[2] as u32 & 0x7F) << 7)
        | (bytes[3] as u32 & 0x7F)
}

pub fn decode_text_frame(data: &[u8]) -> Option<String> {
    if data.is_empty() { return None; }
    match data[0] {
        0 => Some(String::from_utf8_lossy(&data[1..]).trim_matches(char::from(0)).to_string()),
        1 => {
            let utf16: Vec<u16> = data[1..]
                .chunks(2)
                .filter_map(|b| if b.len() == 2 { Some(u16::from_be_bytes([b[0], b[1]])) } else { None })
                .collect();
            Some(String::from_utf16_lossy(&utf16).trim_matches(char::from(0)).to_string())
        }
        _ => None,
    }
}

pub fn parse_vorbis_comments(meta: &mut SongMetadata, data: &[u8]) {
    if data.len() < 8 { return; }
    let vendor_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let mut idx = 4 + vendor_len;
    if idx + 4 > data.len() { return; }
    let count = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
    idx += 4;
    for _ in 0..count {
        if idx + 4 > data.len() { break; }
        let len = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap()) as usize;
        idx += 4;
        if idx + len > data.len() { break; }
        if let Ok(s) = String::from_utf8(data[idx..idx + len].to_vec()) {
            let parts: Vec<_> = s.splitn(2, '=').collect();
            if parts.len() == 2 {
                match parts[0].to_ascii_lowercase().as_str() {
                    "artist" => meta.artist = Some(parts[1].to_string()),
                    "title" => meta.title = Some(parts[1].to_string()),
                    "album" => meta.album = Some(parts[1].to_string()),
                    "genre" => meta.genre = Some(parts[1].to_string()),
                    _ => {}
                }
            }
        }
        idx += len;
    }
}

pub fn extract_m4a_text(data: &[u8]) -> Option<String> {
    let mut i = 0;
    while i + 8 <= data.len() {
        let size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        if size < 8 || i + size > data.len() {
            break;
        }
        if &data[i + 4..i + 8] == b"data" {
            // skip possible data header: often 8 (data header) + 8 (meta) => text starts at i+16
            let start = if i + 16 <= i + size { i + 16 } else { i + 8 };
            let text = String::from_utf8_lossy(&data[start..i + size]);
            return Some(text.trim_matches(char::from(0)).to_string());
        }
        i += size;
    }
    None
}


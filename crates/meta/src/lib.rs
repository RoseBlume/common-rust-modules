mod helpers;

use helpers::{trim_id3v1_text, synchsafe_to_u32, decode_text_frame, parse_vorbis_comments, extract_m4a_text };
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Default)]
pub struct SongMetadata {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub duration_ms: Option<u64>, // ← NEW
}

impl SongMetadata {
    pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_ref = path.as_ref();
        let mut f = File::open(path_ref)?;
        let mut header = [0u8; 12];
        if f.read(&mut header)? < 12 {
            return Ok(Self::default_with_filename(path_ref));
        }
        f.seek(SeekFrom::Start(0))?;

        let mut meta = match &header[0..4] {
            b"RIFF" if &header[8..12] == b"WAVE" => {
                let mut m = Self::from_wav(&mut f)?;
                m.duration_ms = Self::wav_duration(&mut f).ok();
                m
            }
            b"fLaC" => {
                let mut m = Self::from_flac(&mut f)?;
                m.duration_ms = Self::flac_duration(&mut f).ok();
                m
            }
            b"ID3\x03" | b"ID3\x04" => {
                let mut m = Self::from_mp3v2(&mut f)?;
                m.duration_ms = Self::mp3_duration(&mut f).ok();
                m
            }
            _ => {
                // Try MP3v1, M4A, ID3v1 etc.
                let mut m = if let Ok(m1) = Self::from_id3v1(&mut f) {
                    m1
                } else if let Ok(m1) = Self::from_m4a(&mut f) {
                    m1
                } else {
                    SongMetadata::default()
                };
                // attempt M4A duration (if it was m4a) or MP3 duration as fallback
                m.duration_ms = Self::m4a_duration(&mut f).ok().or_else(|| Self::mp3_duration(&mut f).ok());
                m
            }
        };

        // ✅ Automatically assign filename as title if missing
        if meta.title.is_none() {
            meta.title = Some(Self::prettify_filename(path_ref));
        }

        Ok(meta)
    }

    fn default_with_filename(path: &Path) -> Self {
        let mut m = Self::default();
        m.title = Some(Self::prettify_filename(path));
        m
    }

    /// Converts `foo_bar-baz.mp3` → `Foo Bar Baz`
    fn prettify_filename(path: &Path) -> String {
        let file_name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown");

        file_name
            .replace('_', " ")
            .replace('-', " ")
            .split_whitespace()
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    // --- WAV (LIST/INFO) parsing ---
    fn from_wav(f: &mut File) -> io::Result<Self> {
        let mut meta = SongMetadata::default();
        f.seek(SeekFrom::Start(12))?;

        let mut buf = [0u8; 8];
        while f.read(&mut buf)? == 8 {
            let chunk_id = &buf[0..4];
            let chunk_size = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as u64;
            let next = f.seek(SeekFrom::Current(0))? + chunk_size;

            if chunk_id == b"LIST" {
                // Read list type (INFO or others)
                let mut list_type = [0u8; 4];
                f.read_exact(&mut list_type)?;
                if &list_type == b"INFO" {
                    let mut remaining = chunk_size - 4;
                    while remaining >= 8 {
                        let mut sub_header = [0u8; 8];
                        if f.read(&mut sub_header)? != 8 {
                            break;
                        }
                        let sub_id = &sub_header[0..4];
                        let sub_size =
                            u32::from_le_bytes(sub_header[4..8].try_into().unwrap()) as usize;

                        let mut data = vec![0u8; sub_size];
                        f.read_exact(&mut data)?;
                        let text = String::from_utf8_lossy(&data)
                            .trim_matches(char::from(0))
                            .trim()
                            .to_string();

                        match sub_id {
                            b"IART" => meta.artist = Some(text),
                            b"INAM" => meta.title = Some(text),
                            b"IPRD" => meta.album = Some(text),
                            b"IGNR" => meta.genre = Some(text),
                            _ => {}
                        }

                        remaining = remaining.saturating_sub((8 + sub_size) as u64);
                    }
                } else {
                    f.seek(SeekFrom::Start(next))?;
                }
            } else {
                f.seek(SeekFrom::Start(next))?;
            }
        }
        Ok(meta)
    }

    // --- MP3v1 ---
    fn from_id3v1(f: &mut File) -> io::Result<Self> {
        let len = f.seek(SeekFrom::End(0))?;
        if len < 128 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "no id3v1"));
        }
        f.seek(SeekFrom::End(-128))?;
        let mut buf = [0u8; 128];
        f.read_exact(&mut buf)?;
        if &buf[0..3] != b"TAG" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no TAG header"));
        }

        let title = trim_id3v1_text(&buf[3..33]);
        let artist = trim_id3v1_text(&buf[33..63]);
        let album = trim_id3v1_text(&buf[63..93]);
        let genre = Some(format!("{}", buf[127]));

        Ok(SongMetadata {
            artist,
            title,
            album,
            genre,
            duration_ms: None,
        })
    }

    // --- MP3v2 ---
    fn from_mp3v2(f: &mut File) -> io::Result<Self> {
        let mut header = [0u8; 10];
        f.read_exact(&mut header)?;
        if &header[0..3] != b"ID3" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no id3v2 header"));
        }

        let tag_size = synchsafe_to_u32(&header[6..10]) as usize;
        let mut tag_data = vec![0u8; tag_size];
        f.read_exact(&mut tag_data)?;

        let mut meta = SongMetadata::default();
        let mut i = 0;
        while i + 10 <= tag_data.len() {
            let id = &tag_data[i..i + 4];
            let size = u32::from_be_bytes(tag_data[i + 4..i + 8].try_into().unwrap()) as usize;
            if size == 0 || i + 10 + size > tag_data.len() {
                break;
            }
            let frame = &tag_data[i + 10..i + 10 + size];
            let text = decode_text_frame(frame);

            match id {
                b"TIT2" => meta.title = text,
                b"TPE1" => meta.artist = text,
                b"TALB" => meta.album = text,
                b"TCON" => meta.genre = text,
                _ => {}
            }

            i += 10 + size;
        }

        Ok(meta)
    }

    // --- FLAC (Vorbis comment) ---
    fn from_flac(f: &mut File) -> io::Result<Self> {
        let mut header = [0u8; 4];
        f.read_exact(&mut header)?;
        if &header != b"fLaC" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not flac"));
        }

        let mut meta = SongMetadata::default();
        loop {
            let mut block_header = [0u8; 4];
            if f.read(&mut block_header)? != 4 {
                break;
            }

            let last_block = (block_header[0] & 0x80) != 0;
            let block_type = block_header[0] & 0x7F;
            let block_len =
                ((block_header[1] as u32) << 16) | ((block_header[2] as u32) << 8) | block_header[3] as u32;

            if block_type == 4 {
                let mut data = vec![0u8; block_len as usize];
                f.read_exact(&mut data)?;
                parse_vorbis_comments(&mut meta, &data);
            } else {
                f.seek(SeekFrom::Current(block_len as i64))?;
            }

            if last_block {
                break;
            }
        }

        Ok(meta)
    }

    // --- M4A (MP4 atoms) ---
    fn from_m4a(f: &mut File) -> io::Result<Self> {
        let mut meta = SongMetadata::default();
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        let mut i = 0;
        while i + 8 <= data.len() {
            let size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
            if size < 8 || i + size > data.len() {
                break;
            }
            let atom = &data[i + 4..i + 8];
            if atom == b"\xa9nam" {
                meta.title = extract_m4a_text(&data[i + 8..i + size]);
            } else if atom == b"\xa9ART" {
                meta.artist = extract_m4a_text(&data[i + 8..i + size]);
            } else if atom == b"\xa9alb" {
                meta.album = extract_m4a_text(&data[i + 8..i + size]);
            } else if atom == b"\xa9gen" {
                meta.genre = extract_m4a_text(&data[i + 8..i + size]);
            }
            i += size;
        }
        Ok(meta)
    }

    // --- Duration extractors ---

    /// WAV duration in milliseconds (uses byte_rate and data chunk)
    fn wav_duration(f: &mut File) -> io::Result<u64> {
        f.seek(SeekFrom::Start(12))?;

        let mut fmt_found = false;
        let mut byte_rate = 0u32;
        let mut data_size = 0u32;

        let mut buf = [0u8; 8];

        while f.read(&mut buf)? == 8 {
            let id = &buf[0..4];
            let size = u32::from_le_bytes(buf[4..8].try_into().unwrap());
            let next = f.seek(SeekFrom::Current(0))? + size as u64;

            if id == b"fmt " {
                let mut fmt = vec![0u8; size as usize];
                f.read_exact(&mut fmt)?;
                if fmt.len() >= 12 {
                    byte_rate = u32::from_le_bytes(fmt[8..12].try_into().unwrap());
                    fmt_found = true;
                }
            } else if id == b"data" {
                data_size = size;
            } else {
                f.seek(SeekFrom::Start(next))?;
            }
        }

        if fmt_found && byte_rate > 0 {
            let duration_ms = (data_size as u64 * 1000) / byte_rate as u64;
            return Ok(duration_ms);
        }

        Err(io::Error::new(io::ErrorKind::InvalidData, "No WAV duration"))
    }

    /// FLAC duration using STREAMINFO block (total samples / sample rate)
    fn flac_duration(f: &mut File) -> io::Result<u64> {
        f.seek(SeekFrom::Start(4))?;

        // iterate blocks until STREAMINFO (type 0)
        loop {
            let mut block_header = [0u8; 4];
            if f.read(&mut block_header)? != 4 {
                break;
            }
            let last_block = (block_header[0] & 0x80) != 0;
            let block_type = block_header[0] & 0x7F;
            let block_len =
                ((block_header[1] as u32) << 16) | ((block_header[2] as u32) << 8) | block_header[3] as u32;

            if block_type == 0 {
                let mut data = vec![0; block_len as usize];
                f.read_exact(&mut data)?;
                if data.len() < 18 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "STREAMINFO too small"));
                }

                // sample rate: 20 bits (bits 0..19 of the composite field starting at data[10])
                let sample_rate = ((data[10] as u32) << 12)
                    | ((data[11] as u32) << 4)
                    | ((data[12] as u32 & 0xF0) >> 4);

                // total samples: 36 bits (last 4 bits of data[12] and data[13..17])
                let total_samples =
                    ((data[12] as u64 & 0x0F) << 32)
                        | ((data[13] as u64) << 24)
                        | ((data[14] as u64) << 16)
                        | ((data[15] as u64) << 8)
                        | (data[16] as u64);

                if sample_rate == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid sample rate"));
                }

                let duration_ms = (total_samples * 1000) / sample_rate as u64;
                return Ok(duration_ms);
            } else {
                f.seek(SeekFrom::Current(block_len as i64))?;
            }

            if last_block {
                break;
            }
        }

        Err(io::Error::new(io::ErrorKind::InvalidData, "No STREAMINFO"))
    }

    /// M4A/MP4 duration via `mvhd` atom (timescale + duration)
    fn m4a_duration(f: &mut File) -> io::Result<u64> {
        let mut data = Vec::new();
        f.seek(SeekFrom::Start(0))?;
        f.read_to_end(&mut data)?;

        let mut i = 0usize;
        while i + 8 <= data.len() {
            let size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
            if size < 8 || i + size > data.len() {
                break;
            }
            if &data[i + 4..i + 8] == b"moov" {
                // search for mvhd inside moov
                let mut j = i + 8;
                while j + 8 <= i + size {
                    let sub_size = u32::from_be_bytes(data[j..j + 4].try_into().unwrap()) as usize;
                    if sub_size < 8 || j + sub_size > data.len() {
                        break;
                    }
                    if &data[j + 4..j + 8] == b"mvhd" {
                        let version = data[j + 8];
                        if version == 1 {
                            // 64-bit duration: fields at j+24..j+28 timescale, j+28..j+36 duration
                            if j + 36 > data.len() {
                                return Err(io::Error::new(io::ErrorKind::InvalidData, "mvhd truncated"));
                            }
                            let timescale = u32::from_be_bytes(data[j + 24..j + 28].try_into().unwrap());
                            let duration = u64::from_be_bytes(data[j + 28..j + 36].try_into().unwrap());
                            if timescale == 0 {
                                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid timescale"));
                            }
                            return Ok((duration * 1000) / timescale as u64);
                        } else {
                            // version 0: 32-bit duration at j+24..j+28
                            if j + 28 > data.len() {
                                return Err(io::Error::new(io::ErrorKind::InvalidData, "mvhd truncated v0"));
                            }
                            let timescale = u32::from_be_bytes(data[j + 20..j + 24].try_into().unwrap());
                            let duration = u32::from_be_bytes(data[j + 24..j + 28].try_into().unwrap()) as u64;
                            if timescale == 0 {
                                return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid timescale"));
                            }
                            return Ok((duration * 1000) / timescale as u64);
                        }
                    }
                    j += sub_size;
                }
            }
            i += size;
        }

        Err(io::Error::new(io::ErrorKind::NotFound, "No m4a duration"))
    }

    /// MP3 duration: lenient frame scanning that handles VBR/CBR by parsing frames.
    /// This implementation:
    /// - skips ID3v2 tag if present
    /// - then searches for frame sync (0xFFE) and parses headers
    /// - is lenient: if an invalid header is encountered, advance by 1 byte and continue
    /// - sums total samples and derives duration by (total_samples / sample_rate)
    fn mp3_duration(f: &mut File) -> io::Result<u64> {
        use std::cmp::min;

        let total_size = f.metadata()?.len();

        // read whole file into memory chunk-by-chunk for scanning
        f.seek(SeekFrom::Start(0))?;
        let mut all = Vec::with_capacity(min(total_size as usize, 16_000_000));
        f.read_to_end(&mut all)?;

        let mut pos = 0usize;

        // skip ID3v2 if present
        if all.len() >= 10 && &all[0..3] == b"ID3" {
            let tag_size = synchsafe_to_u32(&all[6..10]) as usize;
            pos = 10 + tag_size;
        }

        // helper tables
        let bitrate_table_mpeg1_layer3: [u32; 16] = [0,32,40,48,56,64,80,96,112,128,160,192,224,256,320,0];
        let bitrate_table_mpeg2_layer3: [u32; 16] = [0,8,16,24,32,40,48,56,64,80,96,112,128,144,160,0];

        let mut total_samples: u128 = 0;
        let mut last_sample_rate: u32 = 0;

        // To avoid pathological loops, set a max iterations proportional to file size.
        let max_iterations = all.len() * 2;

        let mut iterations = 0usize;
        while pos + 4 <= all.len() && iterations < max_iterations {
            iterations += 1;

            let b1 = all[pos];
            let b2 = all[pos + 1];

            // sync: 11 bits set -> first byte 0xFF and top 3 bits of second are 1 (0xE0)
            if b1 == 0xFF && (b2 & 0xE0) == 0xE0 {
                if pos + 4 > all.len() {
                    break;
                }
                let header = &all[pos..pos+4];
                let version_bits = (header[1] >> 3) & 0x03;
                let layer_bits = (header[1] >> 1) & 0x03;
                let bitrate_index = (header[2] >> 4) & 0x0F;
                let sample_rate_index = (header[2] >> 2) & 0x03;
                let padding = ((header[2] >> 1) & 0x01) as u32;
                // channel mode (for Xing offset heuristics if needed)
                // let channel_mode = (header[3] >> 6) & 0x03;

                // determine MPEG version
                // 00 -> MPEG 2.5, 01 -> reserved, 10 -> MPEG2, 11 -> MPEG1
                let mpeg_version = match version_bits {
                    0 => 2.5,
                    2 => 2.0,
                    3 => 1.0,
                    _ => {
                        // reserved — treat as invalid
                        pos += 1;
                        continue;
                    }
                };

                // determine layer (we only handle Layer III here; if not layer III try to skip)
                let layer = match layer_bits {
                    1 => 3, // layer III
                    2 => 2,
                    3 => 1,
                    _ => {
                        pos += 1;
                        continue;
                    }
                };

                // We only reliably support Layer III; if not layer III, try to parse generically but be cautious.
                if layer != 3 {
                    // attempt to skip non-layer-III frames: advance by 1 and continue (lenient)
                    pos += 1;
                    continue;
                }

                // sample rate mapping
                let sample_rate = match mpeg_version {
                    1.0 => match sample_rate_index {
                        0 => 44100u32,
                        1 => 48000u32,
                        2 => 32000u32,
                        _ => { pos += 1; continue; }
                    },
                    2.0 => match sample_rate_index {
                        0 => 22050u32,
                        1 => 24000u32,
                        2 => 16000u32,
                        _ => { pos += 1; continue; }
                    },
                    2.5 => match sample_rate_index {
                        0 => 11025u32,
                        1 => 12000u32,
                        2 => 8000u32,
                        _ => { pos += 1; continue; }
                    },
                    _ => { pos += 1; continue; }
                };

                // bitrate (kbps)
                let bitrate_kbps = if mpeg_version == 1.0  {
                    // MPEG1
                    bitrate_table_mpeg1_layer3.get(bitrate_index as usize).copied().unwrap_or(0)
                } else {
                    // MPEG2/2.5
                    bitrate_table_mpeg2_layer3.get(bitrate_index as usize).copied().unwrap_or(0)
                };

                if bitrate_kbps == 0 || sample_rate == 0 {
                    // invalid header values; skip 1 byte and continue (lenient)
                    pos += 1;
                    continue;
                }

                // compute frame length in bytes for Layer III
                // formula:
                // MPEG1 Layer III: frame_size = floor(144000 * bitrate_kbps / sample_rate) + padding
                // MPEG2/2.5 Layer III: frame_size = floor(72000 * bitrate_kbps / sample_rate) + padding
                let frame_size = if mpeg_version == 1.0  {
                    ((144000u32 * bitrate_kbps) / sample_rate) + padding
                } else {
                    ((72000u32 * bitrate_kbps) / sample_rate) + padding
                } as usize;

                if frame_size == 0 {
                    pos += 1;
                    continue;
                }

                // samples per frame
                let samples_per_frame = if mpeg_version == 1.0  {
                    1152u32
                } else {
                    576u32
                };

                // Sanity: ensure we won't overflow and that frame fits
                if pos + frame_size > all.len() {
                    // If frame would extend past EOF, break
                    // but still add the final partial frame's samples proportionally? We'll stop.
                    break;
                }

                // accumulate
                total_samples += samples_per_frame as u128;
                last_sample_rate = sample_rate;
                // advance by frame_size
                pos += frame_size;
            } else {
                // no sync — lenient: advance by 1
                pos += 1;
            }
        }

        // If we parsed frames and have a sample rate, compute duration
        if total_samples > 0 && last_sample_rate > 0 {
            let duration_ms = (total_samples * 1000u128) / (last_sample_rate as u128);
            // clamp to u64
            let duration_u64 = if duration_ms > u128::from(u64::MAX) {
                u64::MAX
            } else {
                duration_ms as u64
            };
            return Ok(duration_u64);
        }

        // fallback: estimate using file size and a typical bitrate (128kbps)
        if total_size > 0 {
            let audio_bytes = total_size;
            let bitrate = 128_000u64; // bits per second
            let duration_ms = (audio_bytes * 8 * 1000) / bitrate;
            return Ok(duration_ms);
        }

        Err(io::Error::new(io::ErrorKind::InvalidData, "Could not determine MP3 duration"))
    }
}



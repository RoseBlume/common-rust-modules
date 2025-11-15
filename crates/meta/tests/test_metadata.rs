use utils::collect_music_files;
use meta::{SongMetadata};
const FILE_PATH: &str = "tests/output/test_reading_metadata.json"; 
#[test]
fn test_reading_metadata() {
    fn is_roman_alphabet(s: &str) -> bool {
        s.chars().all(|c| {
            c.is_ascii_alphabetic() ||
            c.is_ascii_digit() ||
            c.is_ascii_whitespace() ||
            c.is_ascii_punctuation()
        })
    }
    fn write_to_file(entries: Vec<String>) {
        use std::fs::File;
        use std::io::{BufWriter, Write, self};
        let file = File::create(FILE_PATH).expect("Failed to create file");
        let mut writer = BufWriter::new(file);
        for entry in entries {
            writer.write_all(entry.as_bytes()).expect("Failed to write to file");
            writer.flush().expect("Failed to flush file");
        }

    }
    let music_files = collect_music_files();
    let mut file_string: String;
    let mut artist: String;
    let mut title: String;
    let mut album: String;
    let mut genre: String;
    let mut entries: Vec<String> = Vec::new();
    for music_file in music_files {
        
        match music_file.to_str() {
            Some(s) => {
                file_string = s.to_string()
            },
            None => file_string = String::from("File String"),
        }
        let metadata = SongMetadata::from_file(music_file).unwrap();
        let artist = match metadata.artist {
            Some(ref n) if is_roman_alphabet(n) => n.clone(),
            _ => "Unknown Artist".to_string(),
        };

        let album = match metadata.album {
            Some(ref n) if is_roman_alphabet(n) => n.clone(),
            _ => "Unknown Album".to_string(),
        };

        let title = match metadata.title {
            Some(ref n) if is_roman_alphabet(n) => n.clone(),
            _ => "Unknown Title".to_string(),
        };

        let genre = match metadata.genre {
            Some(ref n) if is_roman_alphabet(n) => n.clone(),
            _ => "Unknown Genre".to_string(),
        };
        entries.push(format!(r#"
{{
    "Path": "{file_string}",
    "Artist": "{artist}",
    "Album": "{album}",
    "Title": "{title}",
    "Genre": "{genre}"
}},
"#, 
        artist = artist,
        album = album,
        title = title,
        genre = genre,
        ));


    }
    write_to_file(entries.clone());
    for entry in entries {
        println!("{}",entry);
    }
}
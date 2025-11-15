use std::sync::LazyLock;
use std::path::PathBuf;
use std::fs;
use std::path::Path;
use meta::SongMetadata;
use rand::RandomInt;
use std::fs::File;
use std::io::Write;

#[cfg(target_os = "windows")]
pub static USERNAME: LazyLock<String> = LazyLock::new(|| {
    std::env::var("USERNAME").unwrap_or_else(|_| "unknown".to_string())
});

#[cfg(not(target_os = "windows"))]
pub static USERNAME: LazyLock<String> = LazyLock::new(|| {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
});

pub static MUSIC_FOLDER_PATH: LazyLock<String> = LazyLock::new(|| {
    #[cfg(target_os = "android")]
    {
        "/storage/emulated/0/Music".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        format!("C:\\Users\\{}\\Music", *USERNAME)
    }
    #[cfg(target_os = "linux")]
    {
        format!("/home/{}/Music", *USERNAME)
    }
});

pub static SCANFILE_PATH: LazyLock<String> = LazyLock::new(|| {
    #[cfg(target_os = "windows")]
    {
        format!("C:\\Users\\{}\\AppData\\Local\\Rosary Music\\scan.json", *USERNAME)
    }
    #[cfg(target_os = "macos")]
    {
        format!("/Users/{}/Library/Application Support/RosaryMusic/scan.json", *USERNAME)
    }
    #[cfg(target_os = "linux")]
    {
        format!("/home/{}/.config/Rosary Music/scan.json", *USERNAME)
    }
    #[cfg(target_os = "android")]
    {
        "/storage/emulated/0/Documents/scan.json".to_string()
    }
});

fn collect_music_files() -> Vec<PathBuf> {
    let supported = ["mp3", "m4a", "wav", "flac"];
    // Check if path exists and is a directory
    let path = Path::new(&*MUSIC_FOLDER_PATH);
    
    if !path.exists() {
        eprintln!("Error: Path '{}' does not exist.", &*MUSIC_FOLDER_PATH);
    }
    if !path.is_dir() {
        eprintln!("Error: '{}' is not a directory.", &*MUSIC_FOLDER_PATH);
    }
    
    // Read directory entries
    let mut music_files: Vec<PathBuf> = Vec::new();
    for entry_result in fs::read_dir(path).expect("Failed to read directory") {
        match entry_result {
            Ok(entry) => {
                let file_type = entry.file_type().expect("Could not find file type");
                if file_type.is_file() {
                    let extension:Option<String> = Path::new(&entry.path().display().to_string()).extension()
                         .and_then(|ext| ext.to_str()) // Convert OsStr to &str
                         .map(|ext_str| ext_str.to_lowercase());
                    match extension {
                        Some(n) => {
                            if supported.contains(&n.as_str()) {
                                music_files.push(Path::new(&entry.path().display().to_string()).to_path_buf());
                            }
                            else {
                                #[cfg(debug_assertions)]
                                println!("Skipped File: {}\nFor Reason: Unsupported extension", entry.path().display());
                            }
                        },
                        None => {
                            #[cfg(debug_assertions)]
                            println!("Skipped File: {}\nFor Reason: Unsupported extension", entry.path().display());
                        },
                    }
                    
                } else if file_type.is_dir() {
                    println!("(Skipping directory) {}", entry.path().display());
                } else {
                    println!("(Other) {}", entry.path().display());
                }
            }
            Err(e) => eprintln!("Error reading entry: {}", e),
        }
    }
    music_files
}

pub fn is_roman_alphabet(s: String) -> bool {
    let x = s.as_str();
    x.chars().all(|c| {
        c.is_ascii_alphabetic() ||
        c.is_ascii_digit() ||
        c.is_ascii_whitespace() ||
        c.is_ascii_punctuation()
    })
}
fn scan_music() -> serde_json::Value {
    let music_files: Vec<PathBuf> = collect_music_files();
    let mut file_string: String;
    let mut entries: Vec<serde_json::Value> = Vec::new();
    for music_file in music_files {
        
        match music_file.to_str() {
            Some(s) => {
                file_string = s.to_string()
            },
            None => file_string = String::from("File String"),
        }
        let metadata = SongMetadata::from_file(music_file).unwrap();
        let artist = match metadata.artist {
            Some(ref n) if is_roman_alphabet(n.to_string()) => n.clone(),
            _ => "Unknown Artist".to_string(),
        };

        let album = match metadata.album {
            Some(ref n) if is_roman_alphabet(n.to_string()) => n.clone(),
            _ => "Unknown Album".to_string(),
        };

        let title = match metadata.title {
            Some(ref n) if is_roman_alphabet(n.to_string()) => n.clone(),
            _ => "Unknown Title".to_string(),
        };

        let genre = match metadata.genre {
            Some(ref n) if is_roman_alphabet(n.to_string()) => n.clone().to_string(),
            _ => "Unknown Genre".to_string(),
        };
        let duration = metadata.duration_ms.expect("Failed to get metadata");
        let possible_covers = [1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 14, 15, 16, 17];
        let random_index = RandomInt::new(0, (possible_covers.len() - 1).try_into().unwrap()) as usize;
        let entry = serde_json::json!({
            "location": file_string,
            "artist": artist,
            "album": album,
            "title": title,
            "genre": genre.trim_start_matches(|c: char| !c.is_ascii_alphabetic()),
            "duration": duration,
            "cover":  format!("covers/{}.avif", possible_covers[random_index]),
        });
        

        entries.push(entry);


    }
    serde_json::Value::Array(entries)
}


pub fn get_scan_file() -> serde_json::Value {
 
    let file_path = PathBuf::from(&*SCANFILE_PATH);

    if !file_path.exists() {
        // Run the scan to get music data.
        let scan_data = scan_music();

        // // Ensure the parent directory exists.
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create directory");
        }

        // // Write the JSON data to the file.
        let data_string = serde_json::to_string_pretty(&scan_data)
            .expect("Failed to serialize scan data");
        let mut file = File::create(&file_path)
            .expect("Failed to create scan file");
        file.write_all(data_string.as_bytes())
            .expect("Failed to write scan data");
    }

    // Read and parse the JSON data from file.
    let data_string = fs::read_to_string(&file_path)
        .expect("Failed to read scan file");
    serde_json::from_str(&data_string)
        .expect("Failed to parse scan file JSON")
    
}
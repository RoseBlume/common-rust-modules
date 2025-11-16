use std::sync::LazyLock;
use std::path::PathBuf;
use std::fs;
use std::path::Path;


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

pub fn collect_music_files() -> Vec<PathBuf> {
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



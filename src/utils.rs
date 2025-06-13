pub const IGNORE_DIRS: &[&str] = &[
    ".git",
    ".idea",
    ".vscode",
    "node_modules",
    "dist",
    "target",
    "__pycache__",
    ".pytest_cache",
    "build",
    ".DS_Store",
    ".venv",
    "venv",
    "coder.rs",
];

pub const IGNORE_FILES: &[&str] = &[
    ".DS_Store",
    ".gitignore",
    ".env",
    "package-lock.json",
];

/// Checks if any part of the path matches an ignored directory
pub fn is_ignored_dir(path: &std::path::PathBuf) -> bool {
    path.iter()
        .any(|component| IGNORE_DIRS.contains(&component.to_string_lossy().as_ref()))
}

pub fn chfind(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
        .map(|byte_pos| haystack[..byte_pos].chars().count())
}

pub fn offset_to_point(offset: usize, lines: &str) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;

    for (i, ch) in lines.chars().enumerate() {
        if i == offset {  break }
        if ch == '\n' {
            line += 1; col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

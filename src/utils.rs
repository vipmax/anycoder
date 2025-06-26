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

pub const _IGNORE_FILES: &[&str] = &[
    ".DS_Store",
    ".gitignore",
    ".env",
    "package-lock.json",
];

/// Checks if any part of the path matches an ignored directory
pub fn is_ignored_dir(path: &std::path::PathBuf) -> bool {
    path.iter()
        .any(|p| 
            IGNORE_DIRS.contains(&p.to_string_lossy().as_ref())
        )
}

/// Converts a byte index to a line and column number
pub fn byte_to_point(b: usize, s: &str) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    let mut byte_pos = 0;

    for ch in s.chars() {
        let ch_len = ch.len_utf8();
        if byte_pos + ch_len > b {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        byte_pos += ch_len;
    }

    (line, col)
}

pub fn has_content_changed(old: Option<&String>, new: &str) -> bool {
    match old {
        Some(old_content) => old_content != new,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_byte_to_point_ascii() {
        let text = "hello\nworld";
        assert_eq!(byte_to_point(6, text), (1, 0));
        assert_eq!(byte_to_point(8, text), (1, 2));
    }
    
    #[test]
    fn test_byte_to_point_russian() {
        let text = "привет\nмир";
        assert_eq!(byte_to_point(13, text), (1, 0));
        assert_eq!(byte_to_point(15, text), (1, 1));
        assert_eq!(byte_to_point(6, text), (0, 3));
    }    
}
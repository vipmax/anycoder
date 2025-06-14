use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

pub fn compute_text_edits(old: &str, new: &str) -> Vec<TextEdit> {
    let diff = TextDiff::from_chars(old, new);
    let mut edits: Vec<TextEdit> = Vec::new();

    let mut old_pos = 0;

    for change in diff.iter_all_changes() {
        let value = change.value();

        match change.tag() {
            ChangeTag::Equal => {
                old_pos += value.len();
            }
            ChangeTag::Delete => {
                let start = old_pos;
                let end = start + value.len();

                if let Some(last_edit) = edits.last_mut() {
                    if last_edit.end == start && last_edit.text.is_empty() {
                        last_edit.end = end;
                    } else {
                        edits.push(TextEdit {
                            start, end,
                            text: String::new(),
                        });
                    }
                } else {
                    edits.push(TextEdit {
                        start, end,
                        text: String::new(),
                    });
                }

                old_pos = end;
            }
            ChangeTag::Insert => {
                if let Some(last_edit) = edits.last_mut() {
                    if last_edit.end == old_pos {
                        last_edit.text.push_str(value);
                    } else {
                        edits.push(TextEdit {
                            start: old_pos,
                            end: old_pos,
                            text: value.to_string(),
                        });
                    }
                } else {
                    edits.push(TextEdit {
                        start: old_pos,
                        end: old_pos,
                        text: value.to_string(),
                    });
                }
            }
        }
    }

    edits
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_edits_simple() {
        let before = "let mut foo = 2;\nfoo *= 50;";
        let after =  "let mut foo = 5;\naaaa foo *= 50;";
    
        let edits = compute_text_edits(before, after);
        
        assert_eq!(edits.len(), 2);
        
        assert_eq!(
            edits,
            vec![
                TextEdit { start: 14, end: 15, text: "5".to_string() },
                TextEdit { start: 17, end: 17, text: "aaaa ".to_string() },
            ]
        );    
    }
    
    #[test]
    fn test_compute_edits_simple2() {
        let before = r#"println!("Current value: {}", );"#;
        let after =  r#"println!("Current value: {}", i);"#;
    
        let edits = compute_text_edits(before, after);
        
        assert_eq!(edits, vec![
            TextEdit { start: 30, end: 30, text: "i".to_string() },
        ])    
    }
    
    #[test]
    fn test_compute_edits_unicode() {
        let before = r#"println!("Current значение: {}", i);"#;
        let after =  r#"println!("Current value: {}", i);"#;
    
        let edits = compute_text_edits(before, after);
        
        assert_eq!(edits, vec![
            TextEdit { start: 18, end: 18 + 8*2, text: "value".to_string() },
        ])    
    }
}
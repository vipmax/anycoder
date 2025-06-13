use std::path::PathBuf;
use crate::llm::LlmClient;
use crate::diff::{compute_text_edits, TextEdit};
use serde_json::json;
use crate::prompts::{SYSTEM_PROMPT, REMINDER};
use crate::utils::{ chfind, offset_to_point };


pub const CURSOR_MARKER: &str = "??";
const STOKEN: &str = "<|SEARCH|>";
const DTOKEN: &str = "<|DIVIDE|>";
const RTOKEN: &str = "<|REPLACE|>";
const CTOKEN: &str = "<|cursor|>";

#[derive(Debug)]
pub struct Patch {
    start: usize,
    search: String,
    replace: String,
}

pub struct Coder {
    llm: LlmClient,
}

impl Coder {
    pub fn new(llm: LlmClient) -> Self {
        Self { llm }
    }

    pub async fn autocomplete(
        &self, original: &str, path: &PathBuf, cursor: usize
    ) -> anyhow::Result<String> {

        let context = self.build_context(original, cursor, 3);
        println!("context {:?}", context);

        let big_context = self.build_context(original, cursor, 1000);

        let messages = vec![
            json!({ "role": "system", "content": SYSTEM_PROMPT }),
            json!({ "role": "user", "content": format!("big context:\n{}", big_context.0) }),
            json!({ "role": "user", "content": format!("small context:\n{}", context.0) }),
            json!({ "role": "user", "content": REMINDER }),
        ];

        let response = self.llm.chat(messages).await?;
        println!("response {}", response);

        let patch = self.parse_patch(&response, cursor)?;
        println!("patch {:?}", patch);

        let edits = compute_text_edits(&patch.search, &patch.replace);
        println!("edits {:?}", edits);

        let edits = edits.iter().map(|edit| {
            let s = edit.start + patch.start;
            let e = edit.end + patch.start;
            TextEdit { start: s, end: e, text: edit.text.clone() }
        }).collect::<Vec<_>>();

        let apply_result = self.apply_text_edits(&original, &edits);

        apply_result
    }

    fn build_context(
        &self, original: &str, cursor: usize, context_lines: usize
    ) -> (String, usize) {
        let lines: Vec<&str> = original.lines().collect();

        let (line, col) = offset_to_point(cursor, original);
        let mut cursor_line = line;

        let mut before = context_lines;
        let mut after = context_lines;
        let max_row = lines.len().saturating_sub(1);

        if cursor_line < context_lines {
            after += context_lines - cursor_line;
        } else if cursor_line + context_lines > max_row {
            before += (cursor_line + context_lines) - max_row;
        }

        let start_line = cursor_line.saturating_sub(before);
        let end_line = (cursor_line + after).min(lines.len() - 1);

        let context = lines[start_line..=end_line].join("\n");
        
        let cursor_relative = chfind(&context, CURSOR_MARKER)
            .ok_or_else(|| anyhow::anyhow!(
                "CURSOR_MARKER not found in context, {}", context)
            ).unwrap();
        
        let start = cursor - cursor_relative;

        (
            context.replacen(CURSOR_MARKER, CTOKEN, 1),
            start
        )
    }

    fn parse_patch(
        &self, patch: &str, cursor: usize
    ) -> anyhow::Result<Patch> {
        let search_start = patch.find(STOKEN)
            .ok_or_else(|| anyhow::anyhow!("Invalid patch format: missing <|SEARCH|>"))?;
        let replace_divider = patch.find(DTOKEN)
            .ok_or_else(|| anyhow::anyhow!("Invalid patch format: missing <|DIVIDE|>"))?;
        let replace_end = patch.find(RTOKEN)
            .ok_or_else(|| anyhow::anyhow!("Invalid patch format: missing <|REPLACE|>"))?;

        let search = &patch[search_start + STOKEN.len()..replace_divider];
        let cursor_pos = search.find(CTOKEN);

        let search_no_cursor = search.replace(CTOKEN, "");

        let replace = &patch[replace_divider + DTOKEN.len()..];
        let replace = replace.replace(RTOKEN, "").replace(CTOKEN, "");

        let (start, end) = if let Some(pos) = cursor_pos {
            let before = &search[..pos];
            let after = &search[pos + CTOKEN.len()..];
            let start = cursor.saturating_sub(before.chars().count());
            let end = start + before.chars().count() + after.chars().count();
            (start, end)
        } else {
            let len = search_no_cursor.chars().count();
            (cursor, cursor + len)
        };

        Ok(Patch {
            start,
            search: search_no_cursor,
            replace,
        })
    }

    fn apply_text_edits(
        &self, original: &str, edits: &Vec<TextEdit>
    ) -> anyhow::Result<String> {
        let mut edits = edits.clone();
        // Sort edits by start position in descending order
        // so that applying edits from the end prevents index shifting issues
        edits.sort_by(|a, b| b.start.cmp(&a.start));

        let mut result = original.to_string().replace(CURSOR_MARKER, "");

        for edit in edits {
            // Replace the range [start, end) in the original string with new_text
            // Panics if the starting point or end point do not lie on a char boundary, or if they’re out of bounds.
            if edit.start > result.len() || edit.end > result.len() {
                anyhow::bail!("Edit out of bounds {:?}", edit);
            }else {
                result.replace_range(edit.start..edit.end, &edit.text);
            }
        }

        Ok(result)
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use dotenv::dotenv;

    #[test]
    fn test_build_context_basic() {

        let code = indoc! {r#"
fn main() {
    for i in 0..5 {
        println!("Current value: {}", i);
    }
    println!("end: {}", <|cursor|>);
}
        "#};

        let cursor = chfind(&code, CURSOR_MARKER).unwrap();

        let coder = Coder::new(LlmClient::new("", "", ""));

        let context = coder.build_context(&code, cursor, 1);

        println!("context:\n {:?}", context);

        assert!(context.0.contains(CTOKEN));
        assert!(context.1 == 74);
    }

    #[test]
    fn test_parse_patch() -> anyhow::Result<()> {
        let coder = Coder::new(LlmClient::new("", "", ""));

        let patch = "<|SEARCH|>let <|cursor|> = 10;<|DIVIDE|>let x = 10;<|REPLACE|>";
        let start_pos = 0;

        let parsed = coder.parse_patch(patch, start_pos)?;

        assert_eq!(parsed.start, start_pos);
        assert_eq!(parsed.search, "let  = 10;");
        assert_eq!(parsed.replace, "let x = 10;");

        Ok(())
    }

    #[test]
    fn test_apply_text_edits() -> anyhow::Result<()> {
        let coder = Coder::new(LlmClient::new("", "", ""));
        let original = "The quick brown fox jumps over the lazy dog";

        // Replace "quick" with "slow", "lazy" with "sleepy", and append " and cat"
        let edits = vec![
            TextEdit { start: 43, end: 43, text: " and cat".to_string() },
            TextEdit { start: 35, end: 39, text: "sleepy".to_string() },
            TextEdit { start: 4, end: 9, text: "slow".to_string() },
        ];

        let updated = coder.apply_text_edits(original, &edits)?;

        assert_eq!(
            updated,
            "The slow brown fox jumps over the sleepy dog and cat"
        );
        
        Ok(())
    }

    #[tokio::test]
    async fn test_coder() -> anyhow::Result<()> {
        dotenv()?;

        let api_key = std::env::var("OPENROUTER_API_KEY")?;
        let base_url = "https://openrouter.ai/api/v1";
        let model = "mistralai/codestral-2501";

        let client = LlmClient::new(&api_key, base_url, model);
        let coder = Coder::new(client);

        let code = indoc! {r#"
fn main() {
    for i in 0..5 {
        println!("Current value: {}", i);
    }
}
        "#};

        let cursor = chfind(&code, CURSOR_MARKER).unwrap();

        let path = PathBuf::from("test.rs");

        let newcode = coder.autocomplete(code, &path, cursor).await?;

        println!("newcode:\n{}", newcode);

        Ok(())
    }
}

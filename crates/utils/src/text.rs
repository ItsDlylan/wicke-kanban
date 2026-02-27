use regex::Regex;
use uuid::Uuid;

pub fn git_branch_id(input: &str) -> String {
    // 1. lowercase
    let lower = input.to_lowercase();

    // 2. replace non-alphanumerics with hyphens
    let re = Regex::new(r"[^a-z0-9]+").unwrap();
    let slug = re.replace_all(&lower, "-");

    // 3. trim extra hyphens
    let trimmed = slug.trim_matches('-');

    // 4. take up to 16 chars, then trim trailing hyphens again
    let cut: String = trimmed.chars().take(16).collect();
    cut.trim_end_matches('-').to_string()
}

pub fn short_uuid(u: &Uuid) -> String {
    // to_simple() gives you a 32-char hex string with no hyphens
    let full = u.simple().to_string();
    full.chars().take(4).collect() // grab the first 4 chars
}

pub fn truncate_to_char_boundary(content: &str, max_len: usize) -> &str {
    if content.len() <= max_len {
        return content;
    }

    let cutoff = content
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(content.len()))
        .take_while(|&idx| idx <= max_len)
        .last()
        .unwrap_or(0);

    debug_assert!(content.is_char_boundary(cutoff));
    &content[..cutoff]
}

/// Find the start of a JSON object in text that may contain bare `{` in URLs.
/// Returns the slice from the first `{` that is followed by optional whitespace
/// then `"` (indicating a JSON key), through the last `}`.
pub fn find_json_object_start(s: &str) -> Option<&str> {
    let last_brace = s.rfind('}')?;
    let mut search_from = 0;
    while let Some(pos) = s[search_from..].find('{') {
        let abs_pos = search_from + pos;
        let after = s[abs_pos + 1..].trim_start();
        if after.starts_with('"') {
            return Some(&s[abs_pos..=last_brace]);
        }
        search_from = abs_pos + 1;
    }
    None
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_truncate_to_char_boundary() {
        use super::truncate_to_char_boundary;

        let input = "a".repeat(10);
        assert_eq!(truncate_to_char_boundary(&input, 7), "a".repeat(7));

        let input = "hello world";
        assert_eq!(truncate_to_char_boundary(input, input.len()), input);

        let input = "🔥🔥🔥"; // each fire emoji is 4 bytes
        assert_eq!(truncate_to_char_boundary(input, 5), "🔥");
        assert_eq!(truncate_to_char_boundary(input, 3), "");
    }

    #[test]
    fn test_find_json_object_start() {
        use super::find_json_object_start;

        let s = "some text {\"key\": \"value\"}";
        let result = find_json_object_start(s);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with('{'));
    }

    #[test]
    fn test_find_json_object_start_with_url_braces() {
        use super::find_json_object_start;

        let s = "Route {event}/path\n{\"key\": \"value\"}";
        let result = find_json_object_start(s);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "{\"key\": \"value\"}");
    }
}

pub(crate) fn is_escaped_at(s: &str, byte_idx: usize) -> bool {
    let mut slash_count = 0usize;
    let mut i = byte_idx;
    while i > 0 {
        let Some((prev_idx, ch)) = s[..i].char_indices().next_back() else {
            break;
        };
        if ch != '\\' {
            break;
        }
        slash_count += 1;
        i = prev_idx;
    }
    slash_count % 2 == 1
}

pub(crate) fn find_unescaped_sequence(
    text: &str,
    start_byte: usize,
    needle: &str,
) -> Option<usize> {
    let mut search_start = start_byte;
    while search_start < text.len() {
        let relative = text[search_start..].find(needle)?;
        let byte_idx = search_start + relative;
        if !is_escaped_at(text, byte_idx) {
            return Some(byte_idx);
        }
        search_start = byte_idx + needle.len();
    }
    None
}

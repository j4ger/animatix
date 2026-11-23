use crate::renderer::error::RenderError;

/// Unified line view, with ties emitting deletions before additions.
pub(super) fn line_diff(before: &str, after: &str) -> Result<String, RenderError> {
    let before = before.replace("\r\n", "\n").replace('\r', "\n");
    let after = after.replace("\r\n", "\n").replace('\r', "\n");
    let before: Vec<_> = before.lines().collect();
    let after: Vec<_> = after.lines().collect();
    let stride = after.len() + 1;
    let cells = (before.len() + 1).saturating_mul(stride);
    if cells > 2_000_000 {
        return Err(RenderError::TextCompilation(
            "Code diff is too large for one panel; split the tutorial into smaller source excerpts"
                .into(),
        ));
    }
    let mut common = vec![0u32; cells];
    for (i, a) in before.iter().enumerate().rev() {
        for (j, b) in after.iter().enumerate().rev() {
            let at = i * stride + j;
            common[at] = if a == b {
                common[at + stride + 1] + 1
            } else {
                common[at + stride].max(common[at + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut output = String::new();
    let mut emit = |prefix: char, line: &str| {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push(prefix);
        output.push_str(line);
    };
    while i < before.len() || j < after.len() {
        let at = i * stride + j;
        if i < before.len() && j < after.len() && before[i] == after[j] {
            emit(' ', before[i]);
            i += 1;
            j += 1;
        } else if i < before.len() && (j == after.len() || common[at + stride] >= common[at + 1]) {
            emit('-', before[i]);
            i += 1;
        } else {
            emit('+', after[j]);
            j += 1;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    #[test]
    fn repeats_and_blank_lines_keep_their_order() {
        let diff = super::line_diff("one\r\n\r\nx\r\nx\r\n", "one\n\ny\nx\n").unwrap();
        assert_eq!(diff, " one\n \n-x\n+y\n x");
    }
}

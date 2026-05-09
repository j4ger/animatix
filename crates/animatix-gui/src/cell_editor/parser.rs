use super::Cell;

/// Parse source text into a Vec of cells.
pub fn parse_cells(source: &str) -> Vec<Cell> {
    let lines: Vec<&str> = source.lines().collect();
    let mut cells = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut pending_comments: Vec<String> = Vec::new();
    let mut current_time_s = 0.0_f64;
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        if is_comment_line(trimmed) {
            pending_comments.push(line.to_string());
            i += 1;
            continue;
        }

        if let Some((timestamp, is_relative, brace_kind)) = parse_keyframe_header(trimmed) {
            if !code_lines.is_empty() {
                cells.push(Cell::Code { body: code_lines.join("\n"), expanded: true });
                code_lines.clear();
            }

            let attached_comment = if pending_comments.is_empty() {
                None
            } else {
                Some(pending_comments.join("\n"))
            };
            pending_comments.clear();

            let (body, next_i) = match brace_kind {
                BraceKind::Inline { remainder } => collect_braced_body(&lines, i, remainder),
                BraceKind::NextLine => {
                    if i + 1 < lines.len() && lines[i + 1].trim_start() == "{" {
                        collect_braced_body(&lines, i + 1, "".to_string())
                    } else {
                        collect_legacy_body(&lines, i + 1)
                    }
                }
            };

            let time_s = if is_relative {
                current_time_s += parse_timestamp_seconds(&timestamp);
                current_time_s
            } else {
                current_time_s = parse_timestamp_seconds(&timestamp);
                current_time_s
            };

            cells.push(Cell::Keyframe {
                timestamp,
                is_relative,
                time_s,
                body,
                attached_comment,
            });

            i = next_i;
            continue;
        }

        if !pending_comments.is_empty() {
            code_lines.extend(pending_comments.drain(..));
        }
        code_lines.push(line.to_string());
        i += 1;
    }

    if !pending_comments.is_empty() {
        code_lines.extend(pending_comments.drain(..));
    }

    if !code_lines.is_empty() {
        cells.push(Cell::Code { body: code_lines.join("\n"), expanded: true });
    }

    cells
}

/// Serialize cells back to source text.
pub fn cells_to_source(cells: &[Cell]) -> String {
    cells.iter().map(Cell::to_source).collect()
}

#[derive(Clone, Debug)]
enum BraceKind {
    Inline { remainder: String },
    NextLine,
}

fn is_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("//")
}

fn parse_keyframe_header(trimmed: &str) -> Option<(String, bool, BraceKind)> {
    let header = trimmed.strip_prefix('#')?;
    let brace_idx = header.find('{');
    let timestamp = match brace_idx {
        Some(idx) => header[..idx].trim().to_string(),
        None => header.trim().to_string(),
    };

    if timestamp.is_empty() {
        return None;
    }

    let is_relative = timestamp.starts_with('+');
    let brace_kind = match brace_idx {
        Some(idx) => BraceKind::Inline { remainder: header[idx + 1..].to_string() },
        None => BraceKind::NextLine,
    };

    Some((timestamp, is_relative, brace_kind))
}

fn parse_timestamp_seconds(timestamp: &str) -> f64 {
    let raw = timestamp.trim_start_matches('+').trim();
    if let Some(value) = raw.strip_suffix("ms") {
        value.trim().parse::<f64>().unwrap_or(0.0) / 1000.0
    } else if let Some(value) = raw.strip_suffix('s') {
        value.trim().parse::<f64>().unwrap_or(0.0)
    } else {
        raw.parse::<f64>().unwrap_or(0.0)
    }
}

fn collect_braced_body(lines: &[&str], open_line_idx: usize, first_segment: String) -> (String, usize) {
    let mut body = String::new();
    let mut depth = 1usize;
    let mut line_idx = open_line_idx;
    let mut segment = if first_segment.is_empty() { String::new() } else { first_segment };

    loop {
        let mut chars = segment.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '{' => {
                    depth += 1;
                    body.push(ch);
                }
                '}' => {
                    if depth == 1 {
                        return (body, line_idx + 1);
                    }
                    depth -= 1;
                    body.push(ch);
                }
                _ => body.push(ch),
            }
        }

        line_idx += 1;
        if line_idx >= lines.len() {
            return (body, line_idx);
        }

        // Only add a newline separator when there is already body content.
        // This prevents an extra leading newline when the first segment was
        // empty (e.g. the opening `{` was on its own line).
        if !body.is_empty() {
            body.push('\n');
        }
        segment = lines[line_idx].to_string();
    }
}

fn collect_legacy_body(lines: &[&str], start_idx: usize) -> (String, usize) {
    let mut body_lines = Vec::new();
    let mut idx = start_idx;

    while idx < lines.len() {
        let trimmed = lines[idx].trim_start();
        if parse_keyframe_header(trimmed).is_some() {
            break;
        }
        body_lines.push(lines[idx].to_string());
        idx += 1;
    }

    (body_lines.join("\n"), idx)
}

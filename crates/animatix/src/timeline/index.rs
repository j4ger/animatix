//! Timeline index for bi-directional editor-timeline sync.
//!
//! Maps between source text line numbers and timeline times by scanning
//! the source text and tracking keyframe declarations. Every source line
//! is associated with the "current time" based on the most recent keyframe.

use std::collections::{BTreeMap, HashMap};

/// Index mapping source lines to timeline times and vice versa.
#[derive(Debug, Clone, Default)]
pub struct TimelineIndex {
    /// Maps source line (0-indexed) → absolute time in ms.
    pub line_to_time: HashMap<usize, u64>,
    /// Maps absolute time (ms) → list of source lines active at that time.
    pub time_to_lines: BTreeMap<u64, Vec<usize>>,
    /// Sorted list of (time_ms, line) for keyframe declarations.
    pub keyframes: Vec<(u64, usize)>,
}

impl TimelineIndex {
    /// Build the index by scanning source text line by line.
    ///
    /// Tracks the current time as it encounters keyframe declarations
    /// (`#timestamp` or `#+delta`), then associates every subsequent line
    /// with that time until the next keyframe.
    pub fn build(source: &str) -> Self {
        let mut line_to_time = HashMap::new();
        let mut time_to_lines: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        let mut keyframes = Vec::new();
        let mut current_time_s = 0.0;

        for (line_idx, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();

            // Check for keyframe declaration
            if let Some(after_hash) = trimmed.strip_prefix('#') {
                let after_hash = after_hash.trim_start();
                let is_relative = after_hash.starts_with('+');
                let time_part = if is_relative {
                    &after_hash[1..]
                } else {
                    after_hash
                };

                // Extract the numeric prefix (e.g. "2.5s" or "500ms")
                let num_end = time_part.find(|c: char| !c.is_ascii_digit() && c != '.');
                let num_str = if let Some(end) = num_end {
                    &time_part[..end]
                } else {
                    time_part
                };

                let value: f64 = match num_str.parse() {
                    Ok(v) => v,
                    Err(_) => {
                        // Still associate this line with current time
                        let time_ms = (current_time_s * 1000.0) as u64;
                        line_to_time.insert(line_idx, time_ms);
                        time_to_lines.entry(time_ms).or_default().push(line_idx);
                        continue;
                    },
                };

                let unit = time_part[num_str.len()..].trim_start();
                let delta_s = if unit.starts_with("ms") {
                    value / 1000.0
                } else {
                    value
                };

                if is_relative {
                    current_time_s += delta_s;
                } else {
                    current_time_s = delta_s;
                }

                let time_ms = (current_time_s * 1000.0) as u64;
                keyframes.push((time_ms, line_idx));
            }

            // Associate this line with the current time
            let time_ms = (current_time_s * 1000.0) as u64;
            line_to_time.insert(line_idx, time_ms);
            time_to_lines.entry(time_ms).or_default().push(line_idx);
        }

        keyframes.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            line_to_time,
            time_to_lines,
            keyframes,
        }
    }

    /// Find the time corresponding to a source line.
    pub fn time_for_line(&self, line: usize) -> Option<u64> {
        self.line_to_time.get(&line).copied()
    }

    /// Find the time in seconds for a source line.
    pub fn time_s_for_line(&self, line: usize) -> Option<f64> {
        self.time_for_line(line).map(|ms| ms as f64 / 1000.0)
    }

    /// Find the best source line for a given time.
    ///
    /// Returns the keyframe line that is closest to and ≤ the given time.
    pub fn line_for_time(&self, time_ms: u64) -> Option<usize> {
        self.keyframes.iter().rev().find(|(t, _)| *t <= time_ms).map(|(_, line)| *line)
    }

    /// Find the keyframe time immediately before or at the given time.
    pub fn prev_keyframe_time(&self, time_ms: u64) -> Option<u64> {
        self.keyframes.iter().rev().find(|(t, _)| *t <= time_ms).map(|(t, _)| *t)
    }

    /// Find all source lines associated with a given time.
    pub fn lines_at_time(&self, time_ms: u64) -> Option<&[usize]> {
        self.time_to_lines.get(&time_ms).map(|v| v.as_slice())
    }

    /// Returns true if the given line is a keyframe declaration line.
    pub fn is_keyframe_line(&self, line: usize) -> bool {
        self.keyframes.iter().any(|(_, l)| *l == line)
    }

    /// Returns all keyframe times in seconds.
    pub fn keyframe_times_s(&self) -> Vec<f64> {
        self.keyframes.iter().map(|(ms, _)| *ms as f64 / 1000.0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_maps_lines_to_times() {
        let source = r#"#0s {
    box: Rect, size: (100, 100)
}
#+1s {
    box.color = red
}
#+500ms {
    box.position = (200, 0)
}
"#;

        let index = TimelineIndex::build(source);

        // Line 0: #0s keyframe
        assert_eq!(index.time_s_for_line(0), Some(0.0));
        assert!(index.is_keyframe_line(0));

        // Line 1: actor declaration at t=0
        assert_eq!(index.time_s_for_line(1), Some(0.0));

        // Line 3: #+1s keyframe (relative from 0s → 1s)
        assert_eq!(index.time_s_for_line(3), Some(1.0));
        assert!(index.is_keyframe_line(3));

        // Line 4: assignment at t=1s
        assert_eq!(index.time_s_for_line(4), Some(1.0));

        // Line 6: #+500ms keyframe (relative from 1s → 1.5s)
        assert_eq!(index.time_s_for_line(6), Some(1.5));
        assert!(index.is_keyframe_line(6));
    }

    #[test]
    fn index_finds_line_for_time() {
        let source = r#"#0s {
    box: Rect
}
#+2s {
    box.color = red
}
"#;

        let index = TimelineIndex::build(source);

        // At t=0, should point to line 0 (#0s)
        assert_eq!(index.line_for_time(0), Some(0));

        // At t=500ms, still between 0 and 2s, should point to line 0
        assert_eq!(index.line_for_time(500), Some(0));

        // At t=2s, should point to line 3 (#+2s)
        assert_eq!(index.line_for_time(2000), Some(3));

        // At t=3s, after last keyframe, should point to line 3
        assert_eq!(index.line_for_time(3000), Some(3));
    }

    #[test]
    fn index_handles_absolute_keyframes() {
        let source = r#"#0s { a: Rect }
#2s { a.color = red }
#1s { a.position = (100, 0) }
"#;

        let index = TimelineIndex::build(source);

        // Absolute keyframes: 0s, 2s, 1s (in source order)
        // But keyframes vector should be sorted by time
        assert_eq!(index.keyframes.len(), 3);
        assert_eq!(index.keyframes[0], (0, 0)); // #0s at line 0
        assert_eq!(index.keyframes[1], (1000, 2)); // #1s at line 2
        assert_eq!(index.keyframes[2], (2000, 1)); // #2s at line 1
    }

    #[test]
    fn index_empty_for_no_keyframes() {
        let source = r#"box: Rect, size: (100, 100)
"#;

        let index = TimelineIndex::build(source);

        // No keyframes, but all lines map to time 0
        assert!(index.keyframes.is_empty());
        assert_eq!(index.time_s_for_line(0), Some(0.0));
    }
}

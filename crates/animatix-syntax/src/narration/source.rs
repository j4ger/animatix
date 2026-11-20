use std::{collections::HashMap, ops::Range};

use crate::token::{Token, TokenKind, tokenize};

use super::selector::{fingerprint, normalize, paragraph_id};

/// One standalone narration paragraph from an Animatix document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NarrationParagraph {
    /// Trimmed narration lines joined with a single space.
    pub text: String,
    /// Complete source lines, including leading indentation and the last newline.
    pub span: Range<usize>,
    /// Each trimmed text line's byte range in the source document.
    pub content_spans: Vec<Range<usize>>,
    /// Name of the containing scene, or `None` for the implicit scene.
    pub scene: Option<String>,
    /// Full containing scene byte range, starting at its `# SceneName` line.
    pub scene_span: Range<usize>,
    /// One-based position in the scene among all narration paragraphs.
    pub index: usize,
    /// One-based occurrence of this text in the scene after normalization.
    pub occurrence: usize,
}

impl NarrationParagraph {
    /// Stable identifier through moves around this scene and formatting edits.
    pub fn id(&self) -> String {
        paragraph_id(fingerprint(&normalize(&self.text)), self.occurrence)
    }
}

/// Extract consecutive standalone `///` lines in each scene.
///
/// Inline comments, `////` comments, and comments within brackets, braces
/// or parentheses are ordinary comments. Empty `///` lines separate paragraphs.
pub fn paragraphs(source: &str) -> Vec<NarrationParagraph> {
    paragraphs_with_tokens(source, &tokenize(source))
}

pub(crate) fn paragraphs_with_tokens(source: &str, tokens: &[Token]) -> Vec<NarrationParagraph> {
    let mut result: Vec<NarrationParagraph> = Vec::new();
    let mut depth = 0usize;
    let mut scene = None;
    let mut scene_start = 0;
    let mut first_in_scene = 0;
    for (index, token) in tokens.iter().enumerate() {
        match &token.kind {
            TokenKind::LBrace | TokenKind::LBracket | TokenKind::LParen => depth += 1,
            TokenKind::RBrace | TokenKind::RBracket | TokenKind::RParen => {
                depth = depth.saturating_sub(1);
            },
            TokenKind::Hash if depth == 0 => {
                if let Some(name) = scene_header(&tokens[index + 1..]) {
                    let start = line_start(source, token.span.start);
                    finish_scene(&mut result[first_in_scene..], start);
                    scene = Some(name.to_owned());
                    scene_start = start;
                    first_in_scene = result.len();
                }
            },
            TokenKind::Comment(comment) if depth == 0 && comment.starts_with('/') => {
                let Some((span, content)) = narration_line(source, token) else {
                    continue;
                };
                let text = &source[content.clone()];
                if result.len() > first_in_scene
                    && let Some(previous) = result.last_mut()
                    && previous.span.end == span.start
                {
                    previous.text.push(' ');
                    previous.text.push_str(text);
                    previous.span.end = span.end;
                    previous.content_spans.push(content);
                    continue;
                }
                result.push(NarrationParagraph {
                    text: text.to_owned(),
                    span,
                    content_spans: vec![content],
                    scene: scene.clone(),
                    scene_span: scene_start..source.len(),
                    index: result.len() - first_in_scene + 1,
                    occurrence: 0,
                });
            },
            _ => {}, // Code and ordinary comments do not contribute narration.
        }
    }
    finish_scene(&mut result[first_in_scene..], source.len());
    result
}

fn finish_scene(paragraphs: &mut [NarrationParagraph], end: usize) {
    let mut occurrences = HashMap::new();
    for paragraph in paragraphs {
        paragraph.scene_span.end = end;
        let count = occurrences.entry(normalize(&paragraph.text)).or_insert(0);
        *count += 1;
        paragraph.occurrence = *count;
    }
}

fn scene_header(tokens: &[Token]) -> Option<&str> {
    let mut code = tokens.iter().filter(|token| !matches!(token.kind, TokenKind::Comment(_)));
    let TokenKind::Ident(name) = &code.next()?.kind else {
        return None;
    };
    if matches!(name.as_str(), "step" | "pause")
        || code
            .next()
            .is_some_and(|token| matches!(token.kind, TokenKind::At | TokenKind::LBracket))
    {
        return None;
    }
    Some(name)
}

fn line_start(source: &str, at: usize) -> usize {
    source[..at].rfind('\n').map_or(0, |pos| pos + 1)
}

fn narration_line(source: &str, token: &Token) -> Option<(Range<usize>, Range<usize>)> {
    let start = line_start(source, token.span.start);
    let text_start = token.span.start + 3;
    let raw_text = &source[text_start..token.span.end];
    if !source[start..token.span.start].trim().is_empty() || raw_text.starts_with('/') {
        return None;
    }
    let text = raw_text.trim();
    if text.is_empty() {
        return None;
    }
    let content_start = text_start + raw_text.len() - raw_text.trim_start().len();
    let end = token.span.end + usize::from(source.as_bytes().get(token.span.end) == Some(&b'\n'));
    Some((start..end, content_start..content_start + text.len()))
}

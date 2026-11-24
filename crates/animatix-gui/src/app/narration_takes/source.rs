use std::ops::Range;
use std::path::{Path, PathBuf};

use animatix_syntax::ast::{Expr, Property, Stmt};
use animatix_syntax::narration::{NarrationParagraph, TRACK, paragraphs};
use animatix_syntax::parser::parse_source_complete;
use animatix_syntax::token::{LineIndex, Token, TokenKind};

use super::assets::Pair;
use super::worker::{Adopted, ManifestParagraph};
use crate::app::document::version::SourceEpoch;
use crate::app::stores::SourceStore;
use crate::editor::source_patch::SourcePatch;

const CAPTIONS: &str = "narration_captions";

pub(super) struct Target {
    source: String,
    path: PathBuf,
    epoch: SourceEpoch,
    editor_identity: (u64, u64),
    pub folder: PathBuf,
    pub name: String,
    pub base: Option<(PathBuf, PathBuf)>,
    pub paragraphs: Vec<NarrationParagraph>,
    edits: SourceEdits,
}

impl Target {
    pub fn open(source: &SourceStore, scene: Option<String>) -> Result<Self, String> {
        let text = source.text();
        let path = source.file_path();
        let paragraphs = paragraphs(text)
            .into_iter()
            .filter(|paragraph| paragraph.scene == scene)
            .collect::<Vec<_>>();
        let first = paragraphs
            .first()
            .ok_or("Write a standalone /// narration paragraph in this scene to prepare a take.")?;
        let parsed = parse_source_complete(text);
        if !parsed.errors.is_empty() || !parsed.token_issues.is_empty() {
            return Err("Fix the scene's syntax errors before preparing narration.".into());
        }
        let ast = parsed.ast.ok_or("Could not read the scene.")?;
        let scene_stmts = match scene.as_deref() {
            Some(selected) => ast
                .iter()
                .find_map(|statement| match statement {
                    Stmt::Scene { name, body, .. } if name == selected => Some(body.as_slice()),
                    _ => None,
                })
                .ok_or("The selected scene is no longer present.")?,
            None => ast.as_slice(),
        };

        let folder = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()
            .map_err(|error| format!("Could not open the scene’s folder: {error}"))?;
        let name = scene
            .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
        let (base, edits) =
            SourceEdits::discover(text, &parsed.tokens, scene_stmts, first.span.start, &folder)?;

        Ok(Self {
            source: text.to_owned(),
            path: path.to_owned(),
            epoch: source.epoch(),
            editor_identity: source.editor.source_identity(),
            folder,
            name,
            base,
            paragraphs,
            edits,
        })
    }

    pub fn manifest(&self) -> Vec<ManifestParagraph> {
        self.paragraphs
            .iter()
            .map(|paragraph| ManifestParagraph {
                id: paragraph.id(),
                text: paragraph.text.clone(),
            })
            .collect()
    }

    pub fn is_current(&self, source: &SourceStore) -> bool {
        self.path == source.file_path()
            && self.epoch == source.epoch()
            && self.editor_identity == source.editor.source_identity()
            && self.source == source.text()
            && self.source == source.editor.text()
    }

    pub fn patch(&self, source: &SourceStore, adopted: &Adopted) -> Result<SourcePatch, String> {
        if !self.is_current(source) {
            return Err("The scene changed before voiceover could be updated.".into());
        }
        let audio = relative_literal(&self.folder, &adopted.audio)?;
        let transcript = relative_literal(&self.folder, &adopted.transcript)?;
        let (range, replacement) = self.edits.apply(&self.source, &audio, &transcript)?;
        SourcePatch::new(&source.editor, self.epoch, range, replacement, "Update voiceover")
            .ok_or_else(|| "Could not apply the narration paths to the current source.".into())
    }
}

pub(super) fn source_scene(source: &SourceStore) -> Option<String> {
    let text = source.text();
    let index = LineIndex::new(text);
    source
        .editor
        .cursor_line
        .and_then(|line| {
            let byte = index.line_col_to_byte(line, 0);
            paragraphs(text)
                .into_iter()
                .find(|paragraph| paragraph.span.contains(&byte))
                .map(|paragraph| paragraph.scene)
        })
        .unwrap_or_else(|| source.document.active_scene.clone())
}

pub(super) fn selected_paragraph(source: &SourceStore, target: &Target) -> Option<String> {
    let line = source.editor.cursor_line?;
    let byte = LineIndex::new(source.text()).line_col_to_byte(line, 0);
    target
        .paragraphs
        .iter()
        .find(|paragraph| paragraph.span.contains(&byte))
        .map(NarrationParagraph::id)
}

struct SourceEdits {
    track: String,
    insert_at: usize,
    audio: Option<(Range<usize>, Range<usize>)>,
    legacy_captions: Vec<CaptionEdit>,
    insert_captions: bool,
}

enum CaptionEdit {
    Bind {
        name: Range<usize>,
        value: Range<usize>,
    },
    Clear(Range<usize>),
    Repoint(Range<usize>),
}

impl SourceEdits {
    fn discover(
        source: &str,
        tokens: &[Token],
        statements: &[Stmt],
        fallback: usize,
        folder: &Path,
    ) -> Result<(Option<(PathBuf, PathBuf)>, Self), String> {
        let lines = LineIndex::new(source);
        let first_key = statements.iter().find_map(|statement| match statement {
            Stmt::Keyframe {
                span: Some(span), ..
            }
            | Stmt::RelativeKeyframe {
                span: Some(span), ..
            } => Some(lines.line_col_to_byte(span.start_line.saturating_sub(1), 0)),
            _ => None,
        });
        let mut edits = Self {
            track: TRACK.to_owned(),
            insert_at: first_key.unwrap_or(fallback),
            audio: None,
            legacy_captions: vec![],
            insert_captions: true,
        };
        let mut base = None;
        let existing = match find_actor(statements, TRACK)? {
            Some(actor) => Some(actor),
            None => find_voice(statements)?,
        };
        if let Some((label, ty, props)) = existing {
            if ty != "Audio" {
                return Err(format!("The '{label}' actor must be Audio to bind narration."));
            }
            let (audio, audio_range) = required_literal(props, "source", label)?;
            let (transcript, transcript_range) = required_literal(props, "transcript", label)?;
            base = Some((folder.join(audio), folder.join(transcript)));
            edits.track = label.to_owned();
            edits.audio = Some((audio_range, transcript_range));
        }

        for (label, ty, props) in actors(statements) {
            let bound = ty == "Captions"
                && literal(props, "source").is_some_and(|(track, _)| track == edits.track);
            let old_transcript = if ty == "Captions" {
                literal(props, "transcript").filter(|(path, _)| {
                    base.as_ref().is_some_and(|(_, transcript)| &folder.join(path) == transcript)
                })
            } else {
                None
            };
            if label == CAPTIONS && !bound && old_transcript.is_none() {
                return Err(format!("The '{CAPTIONS}' label is already used by another actor."));
            }
            if bound || old_transcript.is_some() {
                edits.insert_captions = false;
            }
            if let Some((_, range)) = old_transcript {
                let edit = if bound {
                    CaptionEdit::Clear(range)
                } else if props.iter().any(|prop| prop.name == "source") {
                    CaptionEdit::Repoint(range)
                } else {
                    let name = property_name_span(tokens, range.start, "transcript")
                        .ok_or("Could not locate the caption transcript binding.")?;
                    CaptionEdit::Bind { name, value: range }
                };
                edits.legacy_captions.push(edit);
            }
        }

        Ok((base, edits))
    }

    fn apply(
        &self,
        source: &str,
        audio: &str,
        transcript: &str,
    ) -> Result<(Range<usize>, String), String> {
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut writes = Vec::new();
        let mut declarations = String::new();
        if let Some((audio_range, transcript_range)) = &self.audio {
            writes.push((audio_range.clone(), audio.to_owned()));
            writes.push((transcript_range.clone(), transcript.to_owned()));
        } else {
            declarations.push_str(&format!(
                "{TRACK}: Audio, source: {audio}, transcript: {transcript}{newline}",
            ));
        }
        for edit in &self.legacy_captions {
            match edit {
                CaptionEdit::Bind { name, value } => {
                    writes.push((name.clone(), "source".into()));
                    writes.push((value.clone(), format!("\"{}\"", self.track)));
                },
                CaptionEdit::Clear(range) => writes.push((range.clone(), "\"\"".into())),
                CaptionEdit::Repoint(range) => writes.push((range.clone(), transcript.to_owned())),
            }
        }
        if self.insert_captions {
            declarations.push_str(&format!(
                "{CAPTIONS}: Captions, source: \"{}\", opacity: 1{newline}",
                self.track,
            ));
        }
        if !declarations.is_empty() {
            declarations.push_str(newline);
            writes.push((self.insert_at..self.insert_at, declarations));
        }
        writes.sort_by_key(|(range, _)| (range.start, range.end));
        let first = writes.first().ok_or("No narration binding was found.")?.0.start;
        let last = writes.last().ok_or("No narration binding was found.")?.0.end;
        let mut text = String::new();
        let mut cursor = first;
        for (range, value) in writes {
            let unchanged =
                source.get(cursor..range.start).ok_or("Invalid narration source edit.")?;
            text.push_str(unchanged);
            text.push_str(&value);
            cursor = range.end;
        }
        Ok((first..last, text))
    }
}

fn property_name_span(
    tokens: &[Token],
    value_start: usize,
    expected: &str,
) -> Option<Range<usize>> {
    let end = tokens.partition_point(|token| token.span.start < value_start);
    let mut preceding = tokens[..end]
        .iter()
        .rev()
        .filter(|token| !matches!(token.kind, TokenKind::Comment(_)));
    let colon = preceding.next()?;
    let name = preceding.next()?;
    (colon.kind == TokenKind::Colon
        && matches!(&name.kind, TokenKind::Ident(token) if token == expected))
    .then_some(name.span.start..name.span.end)
}

fn relative_literal(folder: &Path, asset: &Path) -> Result<String, String> {
    let asset = asset
        .canonicalize()
        .map_err(|error| format!("Could not read adopted narration: {error}"))?;
    let path = asset
        .strip_prefix(folder)
        .map_err(|_| "Narration tools returned a file outside the scene's folder.")?;
    let parts = path
        .components()
        .map(|part| match part {
            std::path::Component::Normal(name) => {
                name.to_str().filter(|name| !name.contains(['\\', '"', '\n', '\r']))
            },
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("Narration tools returned a path that cannot be written in the scene.")?;
    if parts.is_empty() {
        return Err("Narration tools returned an empty media path.".into());
    }
    Ok(format!("\"{}\"", parts.join("/")))
}

type Actor<'a> = (&'a str, &'a str, &'a [Property]);

fn actors(statements: &[Stmt]) -> impl Iterator<Item = Actor<'_>> {
    statements.iter().filter_map(|statement| match statement {
        Stmt::ActorDecl {
            label,
            ty,
            props,
            array_index: None,
            is_anonymous: false,
            ..
        } => Some((label.as_str(), ty.as_str(), props.as_slice())),
        _ => None,
    })
}

fn find_actor<'a>(statements: &'a [Stmt], label: &str) -> Result<Option<Actor<'a>>, String> {
    let mut matches = actors(statements).filter(|(name, _, _)| *name == label);
    let actor = matches.next();
    if matches.next().is_some() {
        return Err(format!("Multiple '{label}' declarations exist in this scene."));
    }
    Ok(actor)
}

fn find_voice(statements: &[Stmt]) -> Result<Option<Actor<'_>>, String> {
    Ok(find_actor(statements, "voice")?.filter(|(_, ty, props)| {
        *ty == "Audio" && props.iter().any(|prop| prop.name == "transcript")
    }))
}

fn literal<'a>(props: &'a [Property], name: &str) -> Option<(&'a str, Range<usize>)> {
    let mut matches = props.iter().filter(|prop| prop.name == name);
    let prop = matches.next()?;
    let (Expr::Str(value), Some(span)) = (&prop.value, prop.value_span) else {
        return None;
    };
    matches.next().is_none().then_some((value.as_str(), span.start..span.end))
}

fn required_literal<'a>(
    props: &'a [Property],
    name: &str,
    label: &str,
) -> Result<(&'a str, Range<usize>), String> {
    literal(props, name).filter(|(path, _)| !path.is_empty()).ok_or_else(|| {
        format!("Set '{label}.{name}' to a literal file path before replacing its take.")
    })
}

/// Voiceover pairs generated beside a document have safe relative paths and
/// can be copied without rewriting any source literals, including captions.
pub(super) fn managed_pairs(source: &str) -> Vec<Pair> {
    let Some(ast) = parse_source_complete(source).ast else {
        return Vec::new();
    };
    let statements =
        std::iter::once(ast.as_slice()).chain(ast.iter().filter_map(|stmt| match stmt {
            Stmt::Scene { body, .. } => Some(body.as_slice()),
            _ => None,
        }));
    statements
        .flat_map(actors)
        .filter_map(|(_, ty, props)| {
            if ty != "Audio" {
                return None;
            }
            let (audio, _) = literal(props, "source")?;
            let (timing, _) = literal(props, "transcript")?;
            let mut path = Path::new(audio).components();
            if path.next()?.as_os_str() != "narration"
                || !matches!(path.next(), Some(std::path::Component::Normal(_)))
                || path.next().is_some()
            {
                return None;
            }
            let stem = audio.strip_suffix(".wav")?;
            (timing == format!("{stem}.speech.json"))
                .then(|| (PathBuf::from(audio), PathBuf::from(timing)))
        })
        .collect()
}

#[cfg(test)]
mod tests;

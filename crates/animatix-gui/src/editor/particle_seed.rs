//! Source edits for rerolling an emitter at the caret.
use super::EditorBuffer;
use super::source_patch::SourcePatch;
use crate::app::document::version::SourceEpoch;
use animatix_syntax::ast::Stmt;
use std::ops::Range;

impl EditorBuffer {
    /// Only use a current, already parsed revision in the toolbar.
    pub(crate) fn particle_seed_edit(&self) -> Option<(Range<usize>, bool)> {
        let line = self.cursor_line?;
        let ast = self.analyzer.get()?.ast()?;
        let mut edit = None;
        animatix_syntax::walk::walk_stmts(ast, &mut |stmt| {
            let Stmt::ActorDecl {
                ty,
                props,
                span: Some(span),
                ..
            } = stmt
            else {
                return;
            };
            if ty != "Emitter" || !(span.start_line..=span.end_line).contains(&(line + 1)) {
                return;
            }
            if let Some(seed) = props.iter().find(|p| p.name == "seed") {
                if let Some(value) = seed.value_span {
                    edit = Some((value.start..value.end, false));
                }
                return;
            }
            let start = animatix_syntax::token::line_col_to_byte(
                self.text(),
                span.start_line - 1,
                span.start_col - 1,
            );
            let end = animatix_syntax::token::line_col_to_byte(
                self.text(),
                span.end_line - 1,
                span.end_col - 1,
            );
            let Some(decl) = self.text().get(start..end) else {
                return;
            };
            let tokens = animatix_syntax::token::tokenize(decl);
            let mut after_colon = false;
            for tok in tokens {
                let spelling = &decl[tok.span.start..tok.span.end];
                if after_colon && spelling == "Emitter" {
                    let insert = start + tok.span.end;
                    edit = Some((insert..insert, true));
                    break;
                }
                after_colon |= spelling == ":";
            }
        });
        edit
    }

    pub(crate) fn reroll_particle_seed(
        &self,
        epoch: SourceEpoch,
        range: Range<usize>,
        insert: bool,
    ) -> Option<SourcePatch> {
        // A new take is an ordinary, undoable AMX edit. Rendering never uses the clock.
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let mut seed = (time ^ (time >> 32) ^ (time >> 64)) as u32;
        if self.text().get(range.clone())?.parse::<u32>().ok() == Some(seed) {
            seed = seed.wrapping_add(1);
        }
        let value = if insert {
            format!(", seed: {seed}")
        } else {
            seed.to_string()
        };
        SourcePatch::new(self, epoch, range, value, "Reroll particle seed")
    }
}

#[cfg(test)]
mod tests {
    use crate::app::test_support::make_document_store;

    #[test]
    fn reroll_edits_only_the_current_emitter_and_keeps_the_edit_undoable() {
        for original in [
            "// 气泡\n#0s\na: Emitter, rate: 2,\n seed: 7\nother: Emitter, seed: 13\n",
            "// 气泡\n#0s\na: Emitter, rate: 2\nother: Emitter, seed: 13\n",
        ] {
            let mut store = make_document_store(original);
            store.source.editor.cursor_line = Some(2);
            let (range, insert) = store.source.editor.particle_seed_edit().expect("seed target");
            let patch = store
                .source
                .editor
                .reroll_particle_seed(store.source.epoch(), range, insert)
                .unwrap();
            let next = patch.apply(&store.source).unwrap();
            assert!(next.starts_with("// 气泡\n#0s\na: Emitter"));
            assert!(next.ends_with("other: Emitter, seed: 13\n"));
            let (_, errors) = animatix_syntax::parser::parse_source(&next);
            assert!(errors.is_empty(), "{errors:?}");
        }
    }
}

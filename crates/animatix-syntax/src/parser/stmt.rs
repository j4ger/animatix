//!
//! Statement parsers for the Animatix DSL.

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use super::common::{
    self, ExprParser, IdentParser, InlineItemsParser, ModifiersParser, ParserExtra, PropertyParser,
    StmtParser, StrInput,
};
use super::token_parser::*;
use crate::ast::*;
use crate::occurrence::OccurrenceKind;
use crate::token::TokenKind;

/// A type-erased parser over the token stream.
type StmtBoxed<'src, T> = Boxed<'src, 'src, StrInput<'src>, T, ParserExtra<'src>>;

type TypeAnnotationParser<'src> = StmtBoxed<'src, TypeAnnotation>;

type TargetParser<'src> = StmtBoxed<'src, Vec<TargetSegment>>;

/// Parsers shared by several statement builders, built once per recursion.
struct StmtCtx<'src> {
    stmt: StmtParser<'src>,
    expr: ExprParser<'src>,
    property: PropertyParser<'src>,
    modifiers: ModifiersParser<'src>,
    inline_items: InlineItemsParser<'src>,
    ident: IdentParser<'src>,
    type_annotation: TypeAnnotationParser<'src>,
    param_def: StmtBoxed<'src, ParamDef>,
    target: TargetParser<'src>,
    block_props: StmtBoxed<'src, Vec<Property>>,
    let_decl: StmtParser<'src>,
    return_stmt: StmtParser<'src>,
    conditional: StmtParser<'src>,
    match_stmt: StmtParser<'src>,
    for_stmt: StmtParser<'src>,
}

impl<'src> StmtCtx<'src> {
    fn new(
        stmt: StmtParser<'src>,
        expr: ExprParser<'src>,
        property: PropertyParser<'src>,
        modifiers: ModifiersParser<'src>,
        inline_items: InlineItemsParser<'src>,
    ) -> Self {
        let ident = common::ident();
        let type_annotation = type_annotation_parser();
        let param_def = param_def_parser(expr.clone(), type_annotation.clone());
        let target = common::indexed_dotted_ident_with_expr(expr.clone()).boxed();
        let block_props = property
            .clone()
            .separated_by(comma())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(lbrace(), rbrace())
            .boxed();
        let always_body =
            stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace());
        let scoped_always_body = common::scoped(always_body);
        let for_loop_body =
            stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace());
        let scoped_for_loop_body = common::scoped(for_loop_body);
        let let_decl = let_decl_parser(expr.clone());
        let return_stmt = return_stmt_parser(expr.clone());
        let conditional =
            super::control::conditional_stmt(expr.clone(), scoped_always_body.clone()).boxed();
        let match_stmt = super::control::match_stmt(expr.clone(), scoped_always_body).boxed();
        let for_stmt =
            super::control::for_stmt(expr.clone(), modifiers.clone(), scoped_for_loop_body).boxed();
        Self {
            stmt,
            expr,
            property,
            modifiers,
            inline_items,
            ident,
            type_annotation,
            param_def,
            target,
            block_props,
            let_decl,
            return_stmt,
            conditional,
            match_stmt,
            for_stmt,
        }
    }
}

/// Match a type keyword and record it as a [`OccurrenceKind::Type`] occurrence.
fn type_keyword<'src>(
    name: &'static str,
) -> impl Parser<'src, StrInput<'src>, (), ParserExtra<'src>> + Clone {
    common::ident()
        .map_with(|s, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            (s, extra.span())
        })
        .filter(move |(s, _): &(String, ByteSpan)| s == name)
        .map_with(|(s, span), _: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            crate::occurrence::record(OccurrenceKind::Type, s, span);
        })
}

fn type_annotation_parser<'src>() -> TypeAnnotationParser<'src> {
    recursive(|type_annotation| {
        let simple = choice((
            type_keyword("Num").to(TypeAnnotation::Num),
            type_keyword("Str").to(TypeAnnotation::Str),
            type_keyword("Bool").to(TypeAnnotation::Bool),
            type_keyword("Vec2").to(TypeAnnotation::Vec2),
            type_keyword("Vec3").to(TypeAnnotation::Vec3),
            type_keyword("Vec4").to(TypeAnnotation::Vec4),
            type_keyword("Color").to(TypeAnnotation::Color),
            type_keyword("Actor").to(TypeAnnotation::Actor),
            type_keyword("Scene").to(TypeAnnotation::Scene),
            type_keyword("Any").to(TypeAnnotation::Any),
        ));
        let list = type_keyword("List")
            .ignore_then(lt())
            .ignore_then(type_annotation.clone())
            .then_ignore(gt())
            .map(|inner| TypeAnnotation::List(Box::new(inner)));
        let alias_middle = common::ident_occ(OccurrenceKind::Type)
            .filter(|s: &String| s.chars().next().is_some_and(|c| c.is_lowercase() || c == '_'));
        let canonical_alias = common::ident_occ(OccurrenceKind::Type)
            .then(colon_colon().ignore_then(alias_middle.clone()).repeated().collect::<Vec<_>>())
            .then_ignore(colon_colon())
            .then(common::type_ident())
            .map(|((first, middle), last)| {
                let mut parts = vec![first];
                parts.extend(middle);
                parts.push(last);
                TypeAnnotation::Alias(parts.join("::"))
            });
        let legacy_alias = common::ident_occ(OccurrenceKind::Type)
            .then(dot().ignore_then(alias_middle).repeated().collect::<Vec<_>>())
            .then_ignore(dot())
            .then(common::type_ident())
            .map(|((first, middle), last)| {
                let mut parts = vec![first];
                parts.extend(middle);
                parts.push(last);
                TypeAnnotation::Alias(parts.join("::"))
            });
        let tuple = type_keyword("Tuple")
            .ignore_then(lt())
            .ignore_then(
                type_annotation
                    .clone()
                    .separated_by(comma())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .or_not()
                    .map(|items| items.unwrap_or_default()),
            )
            .then_ignore(gt())
            .map(TypeAnnotation::Tuple)
            .boxed();
        let function = type_keyword("Fn")
            .ignore_then(
                type_annotation
                    .clone()
                    .separated_by(comma())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .or_not()
                    .map(|items| items.unwrap_or_default())
                    .delimited_by(lparen(), rparen()),
            )
            .then_ignore(arrow())
            .then(type_annotation.clone())
            .map(|(params, ret)| TypeAnnotation::Function {
                params,
                ret: Box::new(ret),
            })
            .boxed();
        let enum_annotation = type_keyword("Enum")
            .ignore_then(choice((lt().to(()), lparen().to(()))))
            .ignore_then(common::ident().separated_by(comma()).allow_trailing().collect::<Vec<_>>())
            .then_ignore(choice((gt().to(()), rparen().to(()))))
            .map(TypeAnnotation::Enum)
            .boxed();
        let atom = simple
            .or(list)
            .or(tuple)
            .or(function)
            .or(enum_annotation)
            .or(common::type_ident().map(TypeAnnotation::Alias))
            .or(canonical_alias)
            .or(legacy_alias);
        let union = atom
            .clone()
            .then(pipe().ignore_then(atom.clone()).repeated().collect::<Vec<_>>())
            .map(|(first, rest)| {
                if rest.is_empty() {
                    first
                } else {
                    let mut types = vec![first];
                    types.extend(rest);
                    TypeAnnotation::Union(types)
                }
            });
        union.or(atom)
    })
    .boxed()
}

fn param_def_parser<'src>(
    expr: ExprParser<'src>,
    type_annotation: TypeAnnotationParser<'src>,
) -> StmtBoxed<'src, ParamDef> {
    common::ident_decl_occ(OccurrenceKind::Parameter)
        .clone()
        .then(
            colon()
                .ignore_then(
                    type_annotation
                        .clone()
                        .then(assign().ignore_then(expr.clone()).or_not())
                        .map(|(ty, default)| (Some(ty), default))
                        .or(expr.clone().map(|e| (None, Some(e)))),
                )
                .or(assign().ignore_then(expr.clone()).map(|e| (None, Some(e))))
                .or_not()
                .map(|opt| opt.unwrap_or((None, None))),
        )
        .map(|(name, (param_type, default))| ParamDef {
            name,
            param_type,
            default,
        })
        .boxed()
}

fn let_decl_parser<'src>(expr: ExprParser<'src>) -> StmtParser<'src> {
    keyword("pub")
        .or_not()
        .then(
            keyword("let")
                .ignore_then(common::ident_decl_occ(OccurrenceKind::Variable).clone())
                .then_ignore(assign())
                .then(expr),
        )
        .map(|(pub_kw, (name, value))| Stmt::LetDecl {
            is_pub: pub_kw.is_some(),
            name,
            value,
            span: None,
        })
        .labelled("let declaration")
        .as_context()
        .boxed()
}

fn return_stmt_parser<'src>(expr: ExprParser<'src>) -> StmtParser<'src> {
    keyword("return")
        .ignore_then(expr.or_not())
        .map(|value| Stmt::Return { value, span: None })
        .labelled("return statement")
        .as_context()
        .boxed()
}

fn type_alias_parser<'src>(type_annotation: TypeAnnotationParser<'src>) -> StmtParser<'src> {
    keyword("pub")
        .or_not()
        .then(
            keyword("type")
                .ignore_then(common::ident_decl_occ(OccurrenceKind::TypeAlias).clone())
                .then_ignore(assign())
                .then(type_annotation),
        )
        .map(|(is_pub, (name, annotation))| Stmt::TypeAlias {
            is_pub: is_pub.is_some(),
            name,
            annotation,
            span: None,
        })
        .labelled("type alias")
        .as_context()
        .boxed()
}

fn import_stmt_parser<'src>() -> StmtParser<'src> {
    keyword("import")
        .ignore_then(string())
        .then(
            keyword("as")
                .ignore_then(common::ident_decl_occ(OccurrenceKind::ImportAlias).clone())
                .or_not(),
        )
        .map(|(path, alias)| Stmt::Import {
            path,
            alias,
            span: None,
        })
        .labelled("import")
        .as_context()
        .boxed()
}

fn assignment_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    ctx.target
        .clone()
        .then_ignore(assign())
        .then(ctx.expr.clone().map_with(
            |value, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                (value, extra.span())
            },
        ))
        .then(ctx.modifiers.clone())
        .try_map(|((path, (value, value_span)), mut modifiers), span| {
            if path.is_empty() {
                Err(Rich::custom(
                    span,
                    "assignment target must be a property name or actor.property path",
                ))
            } else if path.len() == 1 {
                let property = match &path[0] {
                    TargetSegment::Static(s) => s.clone(),
                    TargetSegment::Indexed { .. } => {
                        return Err(Rich::custom(span, "indexed target cannot be a property name"));
                    },
                };
                let easing = common::extract_easing(&mut modifiers);
                Ok(Stmt::Assignment {
                    target: vec![],
                    property,
                    value,
                    modifiers,
                    easing,
                    value_span: Some(value_span),
                    span: None,
                })
            } else {
                let property = match path.last() {
                    Some(TargetSegment::Static(s)) => s.clone(),
                    Some(TargetSegment::Indexed { .. }) => {
                        return Err(Rich::custom(
                            span,
                            "array index cannot appear on the property segment (e.g. use bars[i].color, not a.b[i])",
                        ));
                    },
                    None => {
                        return Err(Rich::custom(
                            span,
                            "assignment target must include a property name",
                        ));
                    },
                };
                let target = path[..path.len() - 1].to_vec();
                let easing = common::extract_easing(&mut modifiers);
                Ok(Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                    easing,
                    value_span: Some(value_span),
                    span: None,
                })
            }
        })
        .labelled("assignment")
        .as_context()
        .boxed()
}

fn reactive_binding_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    ctx.target
        .clone()
        .then_ignore(reactive_assign())
        .then(ctx.expr.clone().map_with(
            |value, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                (value, extra.span())
            },
        ))
        .try_map(|(path, (value, value_span)), span| {
            if path.len() < 2 {
                Err(Rich::custom(
                    span,
                    "reactive binding target must include an actor label and a property (e.g. 'actor.prop := expr')",
                ))
            } else {
                let property = match path.last() {
                    Some(TargetSegment::Static(s)) => s.clone(),
                    Some(TargetSegment::Indexed { .. }) => {
                        return Err(Rich::custom(
                            span,
                            "array index cannot appear on the property segment (e.g. use bars[i].color, not a.b[i])",
                        ));
                    },
                    None => {
                        return Err(Rich::custom(
                            span,
                            "reactive binding target must include a property name",
                        ));
                    },
                };
                let target = path[..path.len() - 1].to_vec();
                Ok(Stmt::ReactiveBinding {
                    target,
                    property,
                    value,
                    value_span: Some(value_span),
                    span: None,
                })
            }
        })
        .labelled("reactive binding")
        .boxed()
}

/// A labelled shape declaration (`Svg`, `Image`, `Callout`).
fn shape_actor_parser<'src>(
    ctx: &StmtCtx<'src>,
    ty: &'static str,
    default_label: &'static str,
) -> StmtParser<'src> {
    ctx.ident
        .clone()
        .then_ignore(colon())
        .or_not()
        .then_ignore(type_keyword(ty))
        .then(ctx.block_props.clone())
        .map(move |(label, props)| Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.unwrap_or_else(|| default_label.to_string()),
            array_index: None,
            ty: ty.to_string(),
            props,
            modifiers: vec![],
            children: vec![],
            span: None,
        })
        .boxed()
}

/// `label: typst"…"` shorthand for a `Typst` actor.
fn typst_shorthand_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    ctx.ident
        .clone()
        .then_ignore(colon())
        .then(typst())
        .then(ctx.modifiers.clone())
        .map(|((label, content), modifiers)| Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label,
            array_index: None,
            ty: "Typst".to_string(),
            props: vec![Property {
                name: "content".to_string(),
                value: Expr::Str(content),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers,
            children: vec![],
            span: None,
        })
        .boxed()
}

/// `label: "…"` shorthand for a `Text` actor.
fn text_shorthand_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    ctx.ident
        .clone()
        .then_ignore(colon())
        .then(common::string_literal())
        .then(ctx.modifiers.clone())
        .map(|((label, text), modifiers)| Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label,
            array_index: None,
            ty: "Text".to_string(),
            props: vec![Property {
                name: "text".to_string(),
                value: text,
                value_span: None,
                trailing_comment: None,
            }],
            modifiers,
            children: vec![],
            span: None,
        })
        .boxed()
}

fn actor_decl_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    let label_expr = common::label_expr(ctx.expr.clone());
    keyword("pub")
        .or_not()
        .map(|p| p.is_some())
        .then(label_expr)
        .then_ignore(colon())
        .then(common::ident_occ(OccurrenceKind::Type).clone())
        .then(
            comma()
                .ignore_then(ctx.property.clone().separated_by(comma()).collect::<Vec<_>>())
                .or_not()
                .map(|p: Option<Vec<Property>>| p.unwrap_or_default()),
        )
        .then(ctx.modifiers.clone())
        .then(
            ctx.inline_items
                .clone()
                .delimited_by(lbrace(), rbrace())
                .or_not()
                .map(|c| c.unwrap_or_default()),
        )
        .map(|(((((is_pub, (label, array_index)), ty), props), modifiers), children)| {
            Stmt::ActorDecl {
                is_pub,
                is_anonymous: false,
                label,
                array_index,
                ty,
                props,
                modifiers,
                children,
                span: None,
            }
        })
        .labelled("actor declaration")
        .as_context()
        .boxed()
}

/// Parse an action target, accepting one expression index at any path segment.
fn action_target_parser<'src>(
    target: TargetParser<'src>,
) -> StmtBoxed<'src, (String, Option<Expr>, Option<usize>)> {
    target
        .try_map(|segments, span| {
            let count = segments.len();
            let mut key: Vec<String> = Vec::with_capacity(count);
            let mut index = None;
            let mut index_position = None;
            for (pos, segment) in segments.into_iter().enumerate() {
                match segment {
                    TargetSegment::Static(s) => key.push(s),
                    TargetSegment::Indexed {
                        base,
                        index: index_expr,
                    } => {
                        if index.is_some() {
                            return Err(Rich::custom(
                                span,
                                "action targets only support one indexed path segment",
                            ));
                        }
                        key.push(base);
                        index = Some(*index_expr);
                        index_position = Some(pos);
                    },
                }
            }
            Ok((key.join("."), index, index_position))
        })
        .boxed()
}

/// A zero-width check that the parser is not standing on the head of the
/// next statement. `rewind` keeps the matched token in place, so this
/// costs one lookahead and no input.
fn not_a_statement_head<'src>() -> impl Parser<'src, StrInput<'src>, (), ParserExtra<'src>> + Clone
{
    choice((assign(), reactive_assign(), colon())).rewind().not()
}

fn action_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    // Action targets accept one expression index per dotted path: `swap
    // bars[j], row.b[j+1]` projects to string paths plus typed index metadata.
    let action_target = action_target_parser(ctx.target.clone());

    common::ident_occ(OccurrenceKind::Action)
        .clone()
        .then(
            action_target
                .clone()
                .then(
                    comma()
                        .ignore_then(action_target.clone())
                        .repeated()
                        .at_least(1)
                        .collect::<Vec<_>>(),
                )
                .map(|(first, rest)| {
                    let mut targets = vec![first];
                    targets.extend(rest);
                    targets
                })
                // A comma-less list may still hold several targets
                // (`swap bar1 bar2`), but each item has to stop at a
                // statement boundary. The grammar has no newline tokens, so
                // an unguarded repeat runs on into the *next* statement:
                // `persist t` followed by `t.opacity = 0.5` reads
                // `t.opacity` as a second target and then fails on the `=`.
                // A dotted path followed by an assignment or a colon is that
                // next statement's head — an assignment target, or an actor
                // declaration's label — never another action target.
                .or(action_target
                    .clone()
                    .then_ignore(not_a_statement_head())
                    .repeated()
                    .at_least(1)
                    .collect::<Vec<_>>())
                .or_not()
                .map(|opt| opt.unwrap_or_default()),
        )
        // Function-style call args: `highlight_key(bars, key)` — a
        // parenthesized comma-separated list. Plain target-style actions
        // fall back to the existing bare-expr args.
        .then(
            lparen()
                .ignore_then(ctx.expr.clone().separated_by(comma()).collect::<Vec<_>>())
                .then_ignore(rparen())
                // The same boundary rule for the args slot: an expression
                // followed by an assignment operator is the next statement's
                // left-hand side, not an argument. `rewind` keeps that
                // lookahead zero-width, so `repeated` stops without eating the
                // token it was only looking at.
                .or(ctx
                    .expr
                    .clone()
                    .then_ignore(not_a_statement_head())
                    .repeated()
                    .collect::<Vec<_>>()),
        )
        .then(ctx.modifiers.clone())
        .map_with(
            |(((verb, parsed_targets), args), modifiers),
             extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                let (mut targets, mut target_index, mut target_index_position) =
                    (Vec::new(), Vec::new(), Vec::new());
                for (target, index, position) in parsed_targets {
                    targets.push(target);
                    target_index.push(index);
                    target_index_position.push(position);
                }
                Stmt::Action(
                    Action {
                        verb,
                        targets,
                        target_index,
                        target_index_position,
                        args,
                        modifiers,
                        byte_span: Some(extra.span()),
                    },
                    None,
                )
            },
        )
        .boxed()
}

fn sequence_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    keyword("sequence")
        .ignore_then(common::scoped(
            ctx.stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace()),
        ))
        .map(|body| Stmt::Sequence { body, span: None })
        .boxed()
}

fn stagger_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    keyword("stagger")
        .ignore_then(ctx.modifiers.clone())
        .then(common::scoped(
            ctx.stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace()),
        ))
        .map(|(modifiers, body)| Stmt::Stagger {
            modifiers,
            body,
            span: None,
        })
        .boxed()
}

fn always_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    let always_body =
        ctx.stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace());
    keyword("always")
        .ignore_then(common::scoped(always_body))
        .map(|body| Stmt::Always { body, span: None })
        .labelled("always block")
        .as_context()
        .boxed()
}

fn fn_body_parser<'src>(ctx: &StmtCtx<'src>) -> StmtBoxed<'src, Vec<Stmt>> {
    // (let/if/match/for/return) plus an optional trailing expression that
    // is the return value; timeline-function bodies fall back to the full
    // statement grammar (actions, assignments, ...).
    let pure_fn_stmt = choice((
        ctx.let_decl.clone(),
        ctx.conditional.clone(),
        ctx.match_stmt.clone(),
        ctx.for_stmt.clone(),
        ctx.return_stmt.clone(),
    ));
    let pure_fn_body = pure_fn_stmt
        .repeated()
        .collect::<Vec<_>>()
        .then(ctx.expr.clone().or_not())
        .delimited_by(lbrace(), rbrace())
        .map(|(stmts, tail)| {
            let mut body = stmts;
            if let Some(tail) = tail {
                body.push(Stmt::Expr(tail, None));
            }
            body
        });
    pure_fn_body
        .or(ctx.stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace()))
        .boxed()
}

fn fn_decl_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    let fn_body = fn_body_parser(ctx);
    keyword("pub")
        .or_not()
        .map(|p| p.is_some())
        .then_ignore(keyword("fn"))
        .then(common::ident_decl_occ(OccurrenceKind::Action).clone())
        .then(common::scoped(
            ctx.param_def
                .clone()
                .separated_by(comma())
                .collect::<Vec<_>>()
                .delimited_by(lparen(), rparen())
                .or_not()
                .map(|p| p.unwrap_or_default())
                .then(thin_arrow().ignore_then(ctx.type_annotation.clone()).or_not())
                .then(fn_body),
        ))
        .map(|((is_pub, name), ((params, return_type), mut body))| {
            // A timeline function's trailing call expression is a nested
            // function call statement, not a tail value: `fn a() { b() }`
            // means "call b", so convert `Expr(Call(b))` back to an action.
            if return_type.is_none() {
                let tail_call = match body.last() {
                    Some(Stmt::Expr(Expr::Call(verb, args), _)) => {
                        Some((verb.clone(), args.clone()))
                    },
                    _ => None,
                };
                if let Some((verb, args)) = tail_call {
                    body.pop();
                    body.push(Stmt::Action(
                        Action {
                            verb,
                            targets: Vec::new(),
                            target_index: Vec::new(),
                            target_index_position: Vec::new(),
                            args,
                            modifiers: Vec::new(),
                            byte_span: None,
                        },
                        None,
                    ));
                }
            }
            Stmt::FnDecl {
                is_pub,
                name,
                params,
                return_type,
                body,
                span: None,
            }
        })
        .boxed()
}

fn component_def_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    keyword("pub")
        .or_not()
        .map(|p| p.is_some())
        .then_ignore(keyword("component"))
        .then(common::ident_decl_occ(OccurrenceKind::Component).clone())
        .then(common::scoped(
            ctx.param_def
                .clone()
                .separated_by(comma())
                .collect::<Vec<_>>()
                .delimited_by(lparen(), rparen())
                .or_not()
                .map(|p| p.unwrap_or_default())
                .then(
                    ctx.stmt
                        .clone()
                        .repeated()
                        .collect::<Vec<_>>()
                        .delimited_by(lbrace(), rbrace())
                        .try_map(|body: Vec<Stmt>, span| {
                            for stmt in &body {
                                if matches!(stmt, Stmt::Import { .. }) {
                                    return Err(Rich::custom(
                                        span,
                                        "import statements are not allowed inside component bodies",
                                    ));
                                }
                            }
                            Ok(body)
                        }),
                ),
        ))
        .map(|((is_pub, name), (params, body))| {
            Stmt::ComponentDef(
                ComponentDef {
                    is_pub,
                    name,
                    params,
                    body,
                },
                None,
            )
        })
        .labelled("component definition")
        .as_context()
        .boxed()
}

/// Reject `/* */` block comments with a dedicated diagnostic.
fn block_comment_reject<'src>() -> StmtParser<'src> {
    slash()
        .ignore_then(star())
        .try_map(|_, span| {
            Err(Rich::custom(
                span,
                "block comments (/* */) are not supported; use // line comments instead",
            ))
        })
        .boxed()
}

fn step_marker<'src>() -> StmtParser<'src> {
    hash()
        .ignore_then(select! {
            TokenKind::Keyword(k) if k == "step" || k == "pause" => (),
            TokenKind::Ident(i) if i == "step" || i == "pause" => (),
        })
        .then(number().or_not())
        .map(|(_, num)| Stmt::Step {
            index: num.map(|n| n as u64),
            span: None,
        })
        .labelled("step marker")
        .boxed()
}

/// Build the recursive statement parser.
pub(crate) fn parser<'src>(
    expr: ExprParser<'src>,
    property: PropertyParser<'src>,
    modifiers: ModifiersParser<'src>,
    inline_items: InlineItemsParser<'src>,
) -> Boxed<'src, 'src, StrInput<'src>, Stmt, ParserExtra<'src>> {
    recursive(|stmt| {
        let ctx = StmtCtx::new(stmt.boxed(), expr, property, modifiers, inline_items);
        choice((
            block_comment_reject(),
            step_marker(),
            ctx.let_decl.clone(),
            type_alias_parser(ctx.type_annotation.clone()),
            import_stmt_parser(),
            assignment_parser(&ctx),
            reactive_binding_parser(&ctx),
            shape_actor_parser(&ctx, "Svg", "unnamed_svg"),
            shape_actor_parser(&ctx, "Image", "unnamed_image"),
            shape_actor_parser(&ctx, "Callout", "unnamed_callout"),
            always_parser(&ctx),
            ctx.return_stmt.clone(),
            ctx.conditional.clone(),
            ctx.match_stmt.clone(),
            ctx.for_stmt.clone(),
            sequence_parser(&ctx),
            stagger_parser(&ctx),
            fn_decl_parser(&ctx),
            component_def_parser(&ctx),
            typst_shorthand_parser(&ctx),
            text_shorthand_parser(&ctx),
            actor_decl_parser(&ctx),
            action_parser(&ctx),
        ))
        .labelled("statement")
        .boxed()
    })
    .boxed()
}

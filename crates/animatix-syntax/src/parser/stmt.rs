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

pub(super) type TypeAnnotationParser<'src> = StmtBoxed<'src, TypeAnnotation>;

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
    source: common::SourceIndex<'src>,
}

impl<'src> StmtCtx<'src> {
    fn new(
        stmt: StmtParser<'src>,
        expr: ExprParser<'src>,
        property: PropertyParser<'src>,
        modifiers: ModifiersParser<'src>,
        inline_items: InlineItemsParser<'src>,
        source: common::SourceIndex<'src>,
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
            source,
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

pub(super) fn type_annotation_parser<'src>() -> TypeAnnotationParser<'src> {
    recursive(|type_annotation| {
        let simple = choice((
            type_keyword("Num").to(TypeAnnotation::Num),
            type_keyword("Duration").to(TypeAnnotation::Duration),
            type_keyword("Timeline").to(TypeAnnotation::Timeline),
            type_keyword("Unit").to(TypeAnnotation::Unit),
            type_keyword("Str").to(TypeAnnotation::Str),
            type_keyword("Bool").to(TypeAnnotation::Bool),
            type_keyword("Vec2").to(TypeAnnotation::Vec2),
            type_keyword("Vec3").to(TypeAnnotation::Vec3),
            type_keyword("Vec4").to(TypeAnnotation::Vec4),
            type_keyword("Color").to(TypeAnnotation::Color),
            type_keyword("Easing").to(TypeAnnotation::Easing),
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
            .or(canonical_alias);
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
                .ignore_then(type_annotation.clone().map(Some))
                .then(assign().ignore_then(expr.clone()).or_not())
                .map(|(ty, default)| (ty, default))
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
        .try_map(|(path, alias), span| {
            if alias.as_deref() == Some("std") {
                Err(Rich::custom(span, "import alias `std` is reserved for the standard library"))
            } else {
                Ok(Stmt::Import {
                    path,
                    alias,
                    span: None,
                })
            }
        })
        .labelled("import")
        .as_context()
        .boxed()
}

fn assignment_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    let source = ctx.source.clone();
    ctx.target
        .clone()
        .then_ignore(assign())
        .then(ctx.expr.clone().map_with(
            |value, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                (value, extra.span())
            },
        ))
        .then(ctx.modifiers.clone())
        .try_map(move |((path, (value, value_span)), mut modifiers), span| {
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
                    span: common::source_span(&source, span),
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
                    span: common::source_span(&source, span),
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
    let actor_type = common::ident_occ(OccurrenceKind::Type)
        .clone()
        .separated_by(colon_colon())
        .at_least(1)
        .collect::<Vec<_>>()
        .map(|parts| parts.join("::"));
    keyword("pub")
        .or_not()
        .map(|p| p.is_some())
        .then(label_expr)
        .then_ignore(colon())
        .then(actor_type)
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
        .map_with(
            |(
                ((((is_pub, (label, array_index, label_span)), ty), mut props), mut modifiers),
                children,
            ),
             _extra| {
                crate::occurrence::record_declaration(
                    OccurrenceKind::Label,
                    label.clone(),
                    label_span,
                );
                lift_duration_suffix_from_actor_props(&mut props, &mut modifiers);
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
            },
        )
        .labelled("actor declaration")
        .as_context()
        .boxed()
}

/// Recover a duration modifier when the expression parser read the actor's
/// trailing `[duration]` as a postfix index on its final property value.
fn lift_duration_suffix_from_actor_props(props: &mut [Property], modifiers: &mut Vec<Modifier>) {
    let Some(property) = props.last_mut() else {
        return;
    };
    if let Expr::Index(base, index) = property.value.clone()
        && matches!(index.as_ref(), Expr::Duration(_))
    {
        property.value = *base;
        modifiers.insert(
            0,
            Modifier {
                name: None,
                value: *index,
            },
        );
    }
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
    let source = ctx.source.clone();
    // Function bodies can end in any expression; the final one is rewritten
    // as the function's inferred result below.
    choice((
        ctx.stmt.clone(),
        ctx.expr
            .clone()
            .map_with(
                move |expr, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                    let span = common::source_span(&source, extra.span());
                    Stmt::Expr(expr, span)
                },
            )
            .boxed(),
    ))
    .boxed()
    .repeated()
    .collect::<Vec<_>>()
    .delimited_by(lbrace(), rbrace())
    // Expression statements are legal between statements, but the last
    // bare expression in a function body is its inferred return value.
    .map(|mut body| {
        if let Some(Stmt::Expr(expr, span)) = body.last().cloned() {
            body.pop();
            body.push(Stmt::Return {
                value: Some(expr),
                span,
            });
        }
        body
    })
    .boxed()
}

fn fn_decl_parser<'src>(ctx: &StmtCtx<'src>) -> StmtParser<'src> {
    let fn_body = fn_body_parser(ctx);
    keyword("pub")
        .or_not()
        .map(|p| p.is_some())
        .then_ignore(keyword("fn"))
        .then(common::ident_decl_occ(OccurrenceKind::Function).clone())
        .then(common::scoped(
            ctx.param_def
                .clone()
                .separated_by(comma())
                .collect::<Vec<_>>()
                .delimited_by(lparen(), rparen())
                .then(thin_arrow().ignore_then(ctx.type_annotation.clone()).or_not())
                .then(fn_body),
        ))
        .map(|((is_pub, name), ((params, return_type), body))| Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body,
            span: None,
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
    source: common::SourceIndex<'src>,
) -> Boxed<'src, 'src, StrInput<'src>, Stmt, ParserExtra<'src>> {
    recursive(|stmt| {
        let ctx =
            StmtCtx::new(stmt.boxed(), expr, property, modifiers, inline_items, source.clone());
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
            // Stored plan values can be mounted with a bare reference, such
            // as `clip` after `let clip = make_clip()`. Their runtime type is
            // checked when the expression is built.
            {
                let source = ctx.source.clone();
                ctx.expr.clone().try_map(move |expr, byte_span| match expr {
                    Expr::Call(..)
                    | Expr::Timeline(..)
                    | Expr::Sequence(..)
                    | Expr::Stagger { .. }
                    | Expr::Ident(..)
                    | Expr::QualifiedPath(..)
                    | Expr::Field(..)
                    | Expr::Index(..) => {
                        Ok(Stmt::Expr(expr, common::source_span(&source, byte_span)))
                    },
                    _ => Err(Rich::custom(
                        byte_span,
                        "expression statements must be calls or timeline-plan values",
                    )),
                })
            },
        ))
        .labelled("statement")
        .boxed()
    })
    .boxed()
}

use super::super::analyze::Context;
use super::{find_import, project_imported, split_export, walk, ResolvedApi};
use crate::codebase::dependencies::extract::ValueExpression;
use crate::codebase::ts_source::facts::TsFileFacts;
use crate::fx::FxHashSet;
use std::path::{Path, PathBuf};

pub(super) fn classify_local_export(
    ctx: &Context<'_>,
    target: &Path,
    tf: &TsFileFacts,
    local: &str,
    rest: Option<&str>,
    is_target: &impl Fn(&str, &str) -> bool,
    visited: &mut FxHashSet<(PathBuf, String)>,
) -> ResolvedApi {
    if let Some(binding) = tf
        .value_flow
        .bindings
        .iter()
        .find(|b| b.scope == 0 && b.name == local && b.immutable && b.invalidated_at.is_none())
    {
        return classify_value_expr(
            ctx,
            target,
            tf,
            Some(local),
            &binding.value,
            rest,
            is_target,
            visited,
        );
    }
    if is_local_function_or_class(tf, local) {
        return ResolvedApi::Other;
    }
    ResolvedApi::Unanalyzable
}

#[allow(clippy::too_many_arguments)]
fn classify_value_expr(
    ctx: &Context<'_>,
    target: &Path,
    tf: &TsFileFacts,
    local: Option<&str>,
    expr: &ValueExpression,
    rest: Option<&str>,
    is_target: &impl Fn(&str, &str) -> bool,
    visited: &mut FxHashSet<(PathBuf, String)>,
) -> ResolvedApi {
    match expr {
        ValueExpression::Callable(_) => ResolvedApi::Other,
        ValueExpression::Object(fields, _) => rest
            .and_then(|member| {
                let (field, tail) = split_export(member);
                fields.get(field).map(|field_expr| (field_expr, tail))
            })
            .map_or(ResolvedApi::Unanalyzable, |(field_expr, tail)| {
                classify_value_expr(ctx, target, tf, None, field_expr, tail, is_target, visited)
            }),
        ValueExpression::Named { .. } | ValueExpression::Member(..) => {
            follow_expr_alias(ctx, target, tf, expr, rest, is_target, visited)
        }
        ValueExpression::Unknown
            if local.is_some_and(|name| is_local_function_or_class(tf, name)) =>
        {
            ResolvedApi::Other
        }
        _ => ResolvedApi::Unanalyzable,
    }
}

fn follow_expr_alias(
    ctx: &Context<'_>,
    target: &Path,
    tf: &TsFileFacts,
    expr: &ValueExpression,
    rest: Option<&str>,
    is_target: &impl Fn(&str, &str) -> bool,
    visited: &mut FxHashSet<(PathBuf, String)>,
) -> ResolvedApi {
    let mut parts = Vec::new();
    let mut cur = expr;
    while let ValueExpression::Member(inner, prop) = cur {
        parts.push(prop.as_str());
        cur = inner.as_ref();
    }
    let ValueExpression::Named {
        name,
        scope: None | Some(0),
        ..
    } = cur
    else {
        return ResolvedApi::Unanalyzable;
    };
    parts.reverse();
    if let Some(tail) = rest {
        parts.push(tail);
    }
    let suffix = (!parts.is_empty()).then(|| parts.join("."));
    if let Some(imp) = find_import(tf, name) {
        return project_imported(imp, suffix.as_deref()).map_or(
            ResolvedApi::Unanalyzable,
            |next| walk(ctx, target, &imp.specifier, &next, is_target, visited),
        );
    }
    let visit_key = match suffix.as_deref() {
        Some(tail) => format!("\0{name}.{tail}"),
        None => format!("\0{name}"),
    };
    if !visited.insert((target.to_path_buf(), visit_key)) {
        return ResolvedApi::Unanalyzable;
    }
    classify_local_export(ctx, target, tf, name, suffix.as_deref(), is_target, visited)
}

fn is_local_function_or_class(tf: &TsFileFacts, local: &str) -> bool {
    tf.exported_functions.iter().any(|scope| scope == local) || is_local_class_binding(tf, local)
}

fn is_local_class_binding(tf: &TsFileFacts, local: &str) -> bool {
    tf.callable_bindings
        .iter()
        .find(|(scope, name, _)| *scope == 0 && name == local)
        .is_some_and(|(_, _, id)| {
            tf.callable_scope_ids
                .iter()
                .any(|(cid, scope)| cid == id && tf.class_scopes.contains(scope))
        })
}

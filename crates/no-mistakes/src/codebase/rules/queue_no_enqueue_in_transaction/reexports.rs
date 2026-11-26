use super::analyze::Context;
use crate::codebase::dependencies::extract::{
    ExportedBinding, ImportedBinding, ImportedBindingKind,
};
use crate::codebase::dependencies::graph::{ImportTarget, TsFactLookup};
use crate::codebase::ts_source::facts::TsFileFacts;
use crate::fx::FxHashSet;
use std::path::{Path, PathBuf};

#[path = "reexports_local.rs"]
mod local;

pub(super) enum ResolvedApi {
    Target(String, String),
    Other,
    Unanalyzable,
}

pub(super) fn resolve_binding_api(
    ctx: &Context<'_>,
    from: &Path,
    binding: &ImportedBinding,
    suffix: Option<&str>,
    is_target: impl Fn(&str, &str) -> bool,
) -> Option<ResolvedApi> {
    let export = project_imported(binding, suffix)?;
    Some(follow_export(ctx, from, &binding.specifier, &export, &is_target))
}

pub(super) fn follow_export(
    ctx: &Context<'_>,
    from: &Path,
    specifier: &str,
    export: &str,
    is_target: &impl Fn(&str, &str) -> bool,
) -> ResolvedApi {
    walk(ctx, from, specifier, export, is_target, &mut FxHashSet::default())
}

fn walk(
    ctx: &Context<'_>,
    from: &Path,
    specifier: &str,
    export: &str,
    is_target: &impl Fn(&str, &str) -> bool,
    visited: &mut FxHashSet<(PathBuf, String)>,
) -> ResolvedApi {
    if is_target(specifier, export) {
        return ResolvedApi::Target(specifier.to_owned(), export.to_owned());
    }
    let Some(ImportTarget::File(target)) =
        ctx.graph.import_targets.get(from).and_then(|m| m.get(specifier))
    else {
        return if specifier.starts_with('.') || specifier.starts_with('/') {
            ResolvedApi::Unanalyzable
        } else {
            ResolvedApi::Other
        };
    };
    if !visited.insert((target.clone(), export.to_owned())) {
        return ResolvedApi::Unanalyzable;
    }
    let Some(tf) = ctx
        .facts
        .get_ts_facts(target)
        .filter(|f| f.parse_error.is_none() && f.operational_error.is_none())
    else {
        return ResolvedApi::Unanalyzable;
    };
    let (head, rest) = split_export(export);
    let Some(eb) = tf.exported_bindings.iter().find(|eb| eb.exported == head) else {
        return ResolvedApi::Unanalyzable;
    };
    if let Some(next_spec) = &eb.specifier {
        let Some(next_export) = project_export(&eb.local, rest) else {
            return ResolvedApi::Unanalyzable;
        };
        return walk(ctx, target, next_spec, &next_export, is_target, visited);
    }
    let Some(imp) = find_import(tf, &eb.local) else {
        return local::classify_local_export(ctx, target, tf, &eb.local, rest, is_target, visited);
    };
    let Some(next_export) = project_imported(imp, rest) else {
        return ResolvedApi::Unanalyzable;
    };
    walk(ctx, target, &imp.specifier, &next_export, is_target, visited)
}

fn find_import<'a>(tf: &'a TsFileFacts, local: &str) -> Option<&'a ImportedBinding> {
    tf.imported_bindings
        .iter()
        .chain(tf.value_flow.commonjs_imported_bindings.iter())
        .find(|b| !b.is_type_only && b.local == local)
}

fn split_export(export: &str) -> (&str, Option<&str>) {
    export.split_once('.').map_or((export, None), |(h, r)| (h, Some(r)))
}

pub(super) fn binding_reaches_module(
    ctx: &Context<'_>,
    from: &Path,
    binding: &ImportedBinding,
    modules: &FxHashSet<&str>,
) -> bool {
    let export = (binding.kind != ImportedBindingKind::Namespace).then_some(binding.imported.as_str());
    reaches_module(ctx, from, &binding.specifier, export, modules)
}

pub(super) fn reaches_module(
    ctx: &Context<'_>,
    from: &Path,
    specifier: &str,
    export: Option<&str>,
    modules: &FxHashSet<&str>,
) -> bool {
    fn walk(
        ctx: &Context<'_>,
        from: &Path,
        spec: &str,
        export: Option<&str>,
        modules: &FxHashSet<&str>,
        visited: &mut FxHashSet<(PathBuf, Option<String>)>,
    ) -> bool {
        if modules.contains(spec) {
            return true;
        }
        let Some(ImportTarget::File(target)) =
            ctx.graph.import_targets.get(from).and_then(|m| m.get(spec))
        else {
            return false;
        };
        if !visited.insert((target.clone(), export.map(str::to_owned))) {
            return false;
        }
        let Some(tf) = ctx.facts.get_ts_facts(target) else {
            return false;
        };
        let mut exports = tf.exported_bindings.iter().chain(tf.value_flow.type_exports.iter());
        if let Some(name) = export {
            let mut matched = false;
            for eb in exports.filter(|eb| eb.exported == name) {
                matched = true;
                if export_binding_reaches(ctx, target, tf, eb, modules, visited) {
                    return true;
                }
            }
            return !matched
                && tf.star_reexport_specifiers.iter().any(|s| walk(ctx, target, s, Some(name), modules, visited));
        }
        tf.star_reexport_specifiers.iter().any(|s| walk(ctx, target, s, None, modules, visited))
            || exports.any(|eb| export_binding_reaches(ctx, target, tf, eb, modules, visited))
    }

    fn export_binding_reaches(
        ctx: &Context<'_>,
        target: &Path,
        tf: &TsFileFacts,
        eb: &ExportedBinding,
        modules: &FxHashSet<&str>,
        visited: &mut FxHashSet<(PathBuf, Option<String>)>,
    ) -> bool {
        if let Some(next_spec) = &eb.specifier {
            let next_export = (eb.local != "*").then_some(eb.local.as_str());
            return walk(ctx, target, next_spec, next_export, modules, visited);
        }
        tf.imported_bindings
            .iter()
            .chain(tf.value_flow.commonjs_imported_bindings.iter())
            .find(|b| b.local == eb.local)
            .is_some_and(|imp| {
                let next_export = (imp.kind != ImportedBindingKind::Namespace).then_some(imp.imported.as_str());
                walk(ctx, target, &imp.specifier, next_export, modules, visited)
            })
    }
    walk(ctx, from, specifier, export, modules, &mut FxHashSet::default())
}

fn project_export(local: &str, rest: Option<&str>) -> Option<String> {
    match (local, rest) {
        ("*", Some(r)) => Some(r.to_owned()),
        ("*", None) => None,
        (name, Some(r)) => Some(format!("{name}.{r}")),
        (name, None) => Some(name.to_owned()),
    }
}

fn project_imported(imp: &ImportedBinding, rest: Option<&str>) -> Option<String> {
    match (imp.kind, rest) {
        (ImportedBindingKind::Namespace, Some(r)) => Some(r.to_owned()),
        (ImportedBindingKind::Namespace, None) => None,
        (_, Some(r)) => Some(format!("{}.{r}", imp.imported)),
        (_, None) => Some(imp.imported.clone()),
    }
}

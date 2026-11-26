use super::super::analyze::Context;
use super::{local, ResolvedApi};
use crate::codebase::dependencies::extract::{
    ExportedBinding, ImportedBinding, ImportedBindingKind,
};
use crate::codebase::dependencies::graph::{ImportTarget, TsFactLookup};
use crate::codebase::ts_source::facts::TsFileFacts;
use crate::fx::FxHashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum Reachability {
    Reaches,
    Unknown,
    DoesNotReach,
}

impl Reachability {
    pub(in super::super) fn may_reach(self) -> bool {
        !matches!(self, Self::DoesNotReach)
    }

    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Reaches, _) | (_, Self::Reaches) => Self::Reaches,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::DoesNotReach, Self::DoesNotReach) => Self::DoesNotReach,
        }
    }
}

pub(in super::super) fn binding_reaches_module(
    ctx: &Context<'_>,
    from: &Path,
    binding: &ImportedBinding,
    modules: &FxHashSet<&str>,
) -> Reachability {
    let export = (binding.kind != ImportedBindingKind::Namespace).then_some(binding.imported.as_str());
    reaches_module(ctx, from, &binding.specifier, export, modules)
}

pub(in super::super) fn reaches_module(
    ctx: &Context<'_>,
    from: &Path,
    specifier: &str,
    export: Option<&str>,
    modules: &FxHashSet<&str>,
) -> Reachability {
    walk(ctx, from, specifier, export, modules, &mut FxHashSet::default())
}

fn walk(
    ctx: &Context<'_>,
    from: &Path,
    spec: &str,
    export: Option<&str>,
    modules: &FxHashSet<&str>,
    visited: &mut FxHashSet<(PathBuf, Option<String>)>,
) -> Reachability {
    if modules.contains(spec) {
        return Reachability::Reaches;
    }
    let Some(ImportTarget::File(target)) =
        ctx.graph.import_targets.get(from).and_then(|m| m.get(spec))
    else {
        return if spec.starts_with('.') || spec.starts_with('/') {
            Reachability::Unknown
        } else {
            Reachability::DoesNotReach
        };
    };
    if !visited.insert((target.clone(), export.map(str::to_owned))) {
        return Reachability::Unknown;
    }
    let Some(tf) = ctx
        .facts
        .get_ts_facts(target)
        .filter(|f| f.parse_error.is_none() && f.operational_error.is_none())
    else {
        return Reachability::Unknown;
    };
    let exports = tf.exported_bindings.iter().chain(tf.value_flow.type_exports.iter());
    if let Some(name) = export {
        let mut matched = false;
        let mut acc = Reachability::DoesNotReach;
        for eb in exports.filter(|eb| eb.exported == name) {
            matched = true;
            acc = acc.combine(export_binding_reaches(ctx, target, tf, eb, modules, visited));
            if acc == Reachability::Reaches {
                return acc;
            }
        }
        if matched {
            return acc;
        }
        return star_reaches(ctx, target, tf, Some(name), modules, visited);
    }
    let mut acc = star_reaches(ctx, target, tf, None, modules, visited);
    for eb in exports {
        if acc == Reachability::Reaches {
            return acc;
        }
        acc = acc.combine(export_binding_reaches(ctx, target, tf, eb, modules, visited));
    }
    acc
}

fn star_reaches(
    ctx: &Context<'_>,
    target: &Path,
    tf: &TsFileFacts,
    export: Option<&str>,
    modules: &FxHashSet<&str>,
    visited: &mut FxHashSet<(PathBuf, Option<String>)>,
) -> Reachability {
    let mut acc = Reachability::DoesNotReach;
    for spec in &tf.star_reexport_specifiers {
        acc = acc.combine(walk(ctx, target, spec, export, modules, visited));
        if acc == Reachability::Reaches {
            return acc;
        }
    }
    acc
}

fn export_binding_reaches(
    ctx: &Context<'_>,
    target: &Path,
    tf: &TsFileFacts,
    eb: &ExportedBinding,
    modules: &FxHashSet<&str>,
    visited: &mut FxHashSet<(PathBuf, Option<String>)>,
) -> Reachability {
    if let Some(next_spec) = &eb.specifier {
        let next_export = (eb.local != "*").then_some(eb.local.as_str());
        return walk(ctx, target, next_spec, next_export, modules, visited);
    }
    if let Some(imp) = tf
        .imported_bindings
        .iter()
        .chain(tf.value_flow.commonjs_imported_bindings.iter())
        .find(|b| b.local == eb.local)
    {
        let next_export = (imp.kind != ImportedBindingKind::Namespace).then_some(imp.imported.as_str());
        return walk(ctx, target, &imp.specifier, next_export, modules, visited);
    }
    match local::classify_local_export(
        ctx,
        target,
        tf,
        &eb.local,
        None,
        &|spec, _| modules.contains(spec),
        &mut FxHashSet::default(),
    ) {
        ResolvedApi::Target(..) => Reachability::Reaches,
        ResolvedApi::Other => Reachability::DoesNotReach,
        ResolvedApi::Unanalyzable => Reachability::Unknown,
    }
}

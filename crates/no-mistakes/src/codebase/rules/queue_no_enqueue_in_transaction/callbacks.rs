use super::analyze::Context;
use super::builtins::{self, BuiltinCall};
use super::receivers::{is_param_expr, FileEnv};
use super::{finding, unanalyzable};
use crate::codebase::dependencies::extract::{
    CallableId, ImportedBindingKind, ValueCall, ValueExpression,
};
use crate::codebase::dependencies::graph::{ImportTarget, ResolvedCallSite, TsFactLookup};
use crate::codebase::rules::analysis_policy::AnalysisDiagnostics;
use crate::fx::FxHashSet;
use std::path::Path;

pub(super) struct CallbackWalk<'a> {
    pub(super) worklist: Vec<(CallableId, CallableId)>,
    pub(super) visited: FxHashSet<CallableId>,
    pub(super) diagnostics: &'a mut AnalysisDiagnostics,
}

impl CallbackWalk<'_> {
    pub(super) fn inspect_builtin(
        &mut self,
        env: &FileEnv<'_>,
        site: &ResolvedCallSite,
        call: &ValueCall,
        builtin: BuiltinCall,
        tx_owner: CallableId,
    ) {
        for &index in builtin.callbacks {
            if call.spread_at.is_some_and(|spread| spread <= index) {
                self.diagnostics.unanalyzable(env.ctx.policy, unanalyzable(env.ctx, site,
                    "built-in call inside transaction callback has a spread argument at or before its callback slot; pass arguments positionally so callbacks can be checked"));
                return;
            }
            if let Some(arg) = call.arguments.get(index) {
                self.inspect_arg(env, site, arg, builtin.optional_non_callable, tx_owner, 0);
            }
        }
    }

    fn inspect_arg(
        &mut self,
        env: &FileEnv<'_>,
        site: &ResolvedCallSite,
        arg: &ValueExpression,
        optional_non_callable: bool,
        tx_owner: CallableId,
        depth: u8,
    ) {
        if depth > 4 {
            self.emit_unprovable(env, site);
            return;
        }
        match arg {
            ValueExpression::Callable(cb) => {
                if env.file_facts.value_flow.generator_callables.contains(cb) {
                    self.diagnostics.unanalyzable(env.ctx.policy, unanalyzable(env.ctx, site,
                        "built-in callback inside transaction is a generator function, whose execution timing cannot be proved; use a regular or async inline callback"));
                } else if self.visited.insert(*cb) {
                    self.worklist.push((*cb, tx_owner));
                }
            }
            ValueExpression::Undefined
            | ValueExpression::Null
            | ValueExpression::Array(..)
            | ValueExpression::Object(..) => {}
            ValueExpression::Derived(..) | ValueExpression::Unknown if optional_non_callable => {}
            ValueExpression::Named { .. } | ValueExpression::Member(..) => {
                self.inspect_named_or_member(env, site, arg, optional_non_callable, tx_owner, depth);
            }
            _ => self.emit_unprovable(env, site),
        }
    }

    fn inspect_named_or_member(
        &mut self,
        env: &FileEnv<'_>,
        site: &ResolvedCallSite,
        arg: &ValueExpression,
        optional_non_callable: bool,
        tx_owner: CallableId,
        depth: u8,
    ) {
        if is_param_expr(arg, tx_owner, &env.callback_params)
            || builtins::is_builtin_global_fn(arg, &env.imports)
        {
            return;
        }
        if let Some(imported) = resolve_imported_expr(env, arg) {
            match imported {
                ImportedExpr::Publisher(spec, export) => {
                    self.diagnostics
                        .findings
                        .push(finding(env.ctx, site, (spec, export.as_str())));
                }
                ImportedExpr::StarReexport => self.emit_unprovable(env, site),
                ImportedExpr::Other => {}
            }
            return;
        }
        if let ValueExpression::Named { name, scope: Some(scope), .. } = arg {
            if let Some(cb) = env.local_callable(*scope, name) {
                self.inspect_arg(env, site, &ValueExpression::Callable(cb), optional_non_callable, tx_owner, depth + 1);
                return;
            }
            if let Some(bound) = env.local_bindings.get(&(*scope, name.as_str())) {
                if !matches!(bound, ValueExpression::Unknown) {
                    self.inspect_arg(env, site, bound, optional_non_callable, tx_owner, depth + 1);
                    return;
                }
            }
        }
        if !optional_non_callable {
            self.emit_unprovable(env, site);
        }
    }

    fn emit_unprovable(&mut self, env: &FileEnv<'_>, site: &ResolvedCallSite) {
        self.diagnostics.unanalyzable(env.ctx.policy, unanalyzable(env.ctx, site,
            "built-in callback inside transaction cannot be proved not to publish; pass an inline callback or move publishes after commit"));
    }
}

enum ImportedExpr<'a> {
    Publisher(&'a str, String),
    StarReexport,
    Other,
}

fn resolve_imported_expr<'a>(env: &FileEnv<'a>, expr: &ValueExpression) -> Option<ImportedExpr<'a>> {
    let mut parts = Vec::new();
    let mut cur = expr;
    while let ValueExpression::Member(inner, prop) = cur {
        parts.push(prop.as_str());
        cur = inner.as_ref();
    }
    let ValueExpression::Named { name, scope: None | Some(0), .. } = cur else {
        return None;
    };
    let binding = env.imports.get(name.as_str())?;
    parts.reverse();
    let suffix = (!parts.is_empty()).then(|| parts.join("."));
    if has_star_reexport(env.ctx, env.file, &binding.specifier) {
        return Some(ImportedExpr::StarReexport);
    }
    let export = match binding.kind {
        ImportedBindingKind::Namespace => suffix?,
        ImportedBindingKind::Named => suffix.map_or_else(
            || binding.imported.clone(),
            |s| format!("{}.{s}", binding.imported),
        ),
        ImportedBindingKind::Default => suffix.map_or_else(
            || "default".to_string(),
            |s| format!("default.{s}"),
        ),
    };
    if env.ctx.targets.publishers.contains(&(binding.specifier.as_str(), export.as_str())) {
        Some(ImportedExpr::Publisher(binding.specifier.as_str(), export))
    } else {
        Some(ImportedExpr::Other)
    }
}

fn has_star_reexport(ctx: &Context<'_>, from: &Path, specifier: &str) -> bool {
    let Some(ImportTarget::File(target)) =
        ctx.graph.import_targets.get(from).and_then(|m| m.get(specifier))
    else {
        return false;
    };
    ctx.facts
        .get_ts_facts(target)
        .is_some_and(|tf| !tf.star_reexport_specifiers.is_empty())
}

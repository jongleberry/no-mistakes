use super::options::{Api, Targets};
use super::{finding, unanalyzable};
use crate::codebase::check_facts::CheckFactMap;
use crate::codebase::dependencies::extract::{
    CallableId, ImportedBinding, InvocationKind, ValueCall, ValueExpression,
};
use crate::codebase::dependencies::graph::{
    DepGraph, ImportTarget, KnownCallTargetRef, ResolvedCallSite, ResolvedCallTarget, TsFactLookup,
};
use crate::codebase::rules::analysis_policy::AnalysisDiagnostics;
use crate::config::v2::{schema::RuleDef, Unanalyzable};
use crate::fx::{FxHashMap, FxHashSet};
use std::path::Path;

type CallKey<'a> = (u32, &'a str);

pub(super) struct Context<'a> {
    pub(super) root: &'a Path,
    pub(super) application: &'a RuleDef,
    pub(super) policy: Unanalyzable,
    pub(super) targets: &'a Targets<'a>,
    pub(super) graph: &'a DepGraph,
    pub(super) facts: &'a CheckFactMap,
}

pub(super) fn check_file(ctx: &Context<'_>, file: &Path, diagnostics: &mut AnalysisDiagnostics) {
    let Some(file_facts) = ctx.facts.get_ts_facts(file)
        .filter(|f| f.parse_error.is_none() && f.operational_error.is_none()) else { return; };
    let sites = ctx.graph.call_sites_in_file(file);
    let flow = &file_facts.value_flow;
    let imports: FxHashMap<&str, &ImportedBinding> = file_facts.imported_bindings.iter()
        .filter(|b| !b.is_type_only).map(|b| (b.local.as_str(), b)).collect();
    let calls: FxHashMap<CallKey<'_>, &ValueCall> = flow.calls.iter()
        .filter(|c| !flow.tagged_templates.contains_key(&(c.offset, c.end)))
        .map(|c| ((c.offset, c.source_callee.as_str()), c)).collect();
    let mut callbacks = FxHashSet::default();
    for site in sites.iter().filter(|site| site.invocation == InvocationKind::Call) {
        if let Some(index) = exact_target(site).and_then(|api| ctx.targets.transactions.get(&api).copied()) {
            match callback_for_transaction(site, index, &calls, &imports, &flow.generator_callables) {
                Ok(callback) => { callbacks.insert(callback); }
                Err(reason) => diagnostics.unanalyzable(ctx.policy, unanalyzable(ctx, site, reason)),
            }
            continue;
        }
        if is_ambiguous(site) && any_candidate_matches(site, |api| ctx.targets.transactions.contains_key(&api)) {
            diagnostics.unanalyzable(ctx.policy, unanalyzable(ctx, site,
                "ambiguous call target may invoke a configured transaction; call the imported transaction directly"));
        } else if site.target == ResolvedCallTarget::Unknown
            && unresolved_transaction_site(ctx, file, site, calls.get(&site_key(site)).copied(), &imports)
        {
            diagnostics.unanalyzable(ctx.policy, unanalyzable(ctx, site,
                "unresolved call target may invoke a configured transaction (dynamic member, factory return, or wildcard re-export); import and call the transaction directly"));
        }
    }
    let callback_params: FxHashSet<(CallableId, usize, &str)> = flow
        .parameters
        .iter()
        .filter(|param| callbacks.contains(&param.owner))
        .map(|param| (param.owner, param.scope, param.name.as_str()))
        .collect();
    for site in sites.iter().filter(|site| {
        site.invocation == InvocationKind::Call
            && !flow.deferred_calls.contains(&site.offset)
            && site.caller_id.is_some_and(|caller| callbacks.contains(&caller))
    }) {
        if let Some(api) = exact_target(site) {
            if ctx.targets.publishers.contains(&api)
                && !calls.get(&site_key(site)).is_some_and(|call| call.bound)
            {
                diagnostics.findings.push(finding(ctx, site, api));
            }
        } else if is_ambiguous(site)
            && (site.has_unknown_target() || any_candidate_matches(site, |api| ctx.targets.publishers.contains(&api)))
        {
            diagnostics.unanalyzable(ctx.policy, unanalyzable(ctx, site,
                "ambiguous call target inside transaction callback may invoke a configured publisher; call a single known target or move publishes after commit"));
        } else if site.target == ResolvedCallTarget::Unknown
            && !site.caller_id.is_some_and(|caller| {
                is_callback_param_member(calls.get(&site_key(site)).copied(), caller, &callback_params)
            })
        {
            diagnostics.unanalyzable(ctx.policy, unanalyzable(ctx, site,
                "unresolved call target inside transaction callback cannot be proved not to publish; use a statically resolvable call or move publishes after commit"));
        }
    }
}

fn callback_for_transaction(
    site: &ResolvedCallSite,
    index: usize,
    calls: &FxHashMap<CallKey<'_>, &ValueCall>,
    imports: &FxHashMap<&str, &ImportedBinding>,
    generators: &FxHashSet<CallableId>,
) -> Result<CallableId, &'static str> {
    let Some(call) = calls.get(&site_key(site)) else {
        return Err("transaction call uses a tagged template, so callback argument position cannot be proved; call the transaction directly with an inline callback");
    };
    if call.bound
        || !site.returned_from.is_empty()
        || !site.returned_member_from.is_empty()
        || !root_ident(&call.callee).is_some_and(|name| imports.contains_key(name))
    {
        return Err("transaction call is not a direct import binding (for example bound, factory-returned, or aliased), so argument positions cannot be proved; call the imported transaction directly");
    }
    if call.spread_at.is_some_and(|spread| spread <= index) {
        return Err("transaction call has a spread argument at or before the callback slot; pass arguments positionally with an inline callback");
    }
    let Some(ValueExpression::Callable(callback)) = call.arguments.get(index) else {
        return Err("transaction callback argument is not an inline function expression; pass an inline arrow or function expression");
    };
    if generators.contains(callback) {
        return Err("transaction callback is a generator function, whose execution timing cannot be proved; use a regular or async inline callback");
    }
    Ok(*callback)
}

fn exact_target(site: &ResolvedCallSite) -> Option<Api<'_>> {
    if !site.alternatives.is_empty() { return None; }
    match &site.target {
        ResolvedCallTarget::ModuleExport { specifier, export_path, .. } => Some((specifier, export_path)),
        _ => None,
    }
}

fn is_ambiguous(site: &ResolvedCallSite) -> bool {
    !site.alternatives.is_empty() || matches!(site.target, ResolvedCallTarget::Alternatives(_))
}

fn any_candidate_matches(site: &ResolvedCallSite, predicate: impl Fn(Api<'_>) -> bool + Copy) -> bool {
    site.targets().any(|candidate| match candidate.target {
        KnownCallTargetRef::ModuleExport { specifier, export_path, .. } => predicate((specifier, export_path)),
        _ => false,
    }) || site.alternatives.iter().any(|alt| any_candidate_matches(alt, predicate))
}

fn unresolved_transaction_site(
    ctx: &Context<'_>,
    file: &Path,
    site: &ResolvedCallSite,
    call: Option<&ValueCall>,
    imports: &FxHashMap<&str, &ImportedBinding>,
) -> bool {
    let mods = &ctx.targets.transaction_modules;
    let txs = &ctx.targets.transactions;
    if site.returned_from.iter().any(|(m, e)| mods.contains(m.as_str()) && !txs.contains_key(&(m.as_str(), e.as_str())))
        || site.returned_member_from.iter().any(|(m, e, _)| mods.contains(m.as_str()) && !txs.contains_key(&(m.as_str(), e.as_str())))
    {
        return true;
    }
    let Some(binding) = call.and_then(|c| root_ident(&c.callee)).and_then(|name| imports.get(name)) else {
        return false;
    };
    let mut visited = FxHashSet::default();
    specifier_reaches_transaction(ctx, file, &binding.specifier, &mut visited)
}

fn specifier_reaches_transaction(
    ctx: &Context<'_>,
    from: &Path,
    specifier: &str,
    visited: &mut FxHashSet<std::path::PathBuf>,
) -> bool {
    if ctx.targets.transaction_modules.contains(specifier) { return true; }
    let Some(ImportTarget::File(target)) = ctx.graph.import_targets.get(from).and_then(|m| m.get(specifier)) else {
        return false;
    };
    if !visited.insert(target.clone()) { return false; }
    let Some(tf) = ctx.facts.get_ts_facts(target) else { return false; };
    tf.star_reexport_specifiers.iter().any(|s| specifier_reaches_transaction(ctx, target, s, visited))
}

fn site_key(site: &ResolvedCallSite) -> CallKey<'_> { (site.offset, site.source_callee.as_str()) }

fn root_ident(mut expr: &ValueExpression) -> Option<&str> {
    while let ValueExpression::Member(receiver, _) = expr { expr = receiver; }
    match expr {
        ValueExpression::Named { name, scope: None | Some(0), .. } => Some(name.as_str()),
        _ => None,
    }
}

fn is_callback_param_member(
    call: Option<&ValueCall>,
    caller: CallableId,
    params: &FxHashSet<(CallableId, usize, &str)>,
) -> bool {
    let Some(ValueExpression::Member(receiver, _)) = call.map(|c| &c.callee) else { return false; };
    let mut receiver = receiver.as_ref();
    while let ValueExpression::Member(inner, _) = receiver { receiver = inner.as_ref(); }
    matches!(receiver, ValueExpression::Named { name, scope: Some(scope), .. }
        if params.contains(&(caller, *scope, name.as_str())))
}

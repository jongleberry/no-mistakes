use crate::codebase::check_facts::CheckFactMap;
use crate::codebase::dependencies::extract::{CallableId, InvocationKind, ValueCall, ValueExpression};
use crate::codebase::dependencies::graph::{
    DepGraph, ResolvedCallSite, ResolvedCallTarget, TsFactLookup,
};
use crate::codebase::rules::RuleFinding;
use crate::codebase::ts_resolver::normalize_path;
use crate::codebase::ts_source::relative_slash_path;
use crate::config::v2::schema::{NoMistakesConfig, RuleDef};
use anyhow::Result;
use crate::fx::{FxHashMap, FxHashSet};
use std::path::Path;

mod options;
use options::{Api, Options};

pub const RULE_ID: &str = "queue-no-enqueue-in-transaction";

pub(crate) fn check(
    root: &Path,
    config: &NoMistakesConfig,
    graph: &DepGraph,
    facts: &CheckFactMap,
) -> Result<Vec<RuleFinding>> {
    let mut findings = Vec::new();
    let root = normalize_path(root);
    for application in config.rule_applications(RULE_ID) {
        let options = application.try_rule_options::<Options>()?;
        let targets = options.targets()?;
        let files = super::selected_rule_files(
            &root,
            config,
            application,
            facts.graph_file_universe(),
        )?;
        for file in files {
            let file = normalize_path(&file);
            let sites = graph.call_sites_in_file(&file);
            let wrappers: FxHashMap<_, _> = sites
                .iter()
                .filter_map(|site| Some((site_key(site), *targets.transactions.get(&target(site)?)?)))
                .collect();
            if wrappers.is_empty() {
                continue;
            }
            let Some(facts) = facts.get_ts_facts(&file).filter(|facts| {
                facts.parse_error.is_none() && facts.operational_error.is_none()
            }) else {
                continue;
            };
            let flow = &facts.value_flow;
            let imports: FxHashSet<_> = facts
                .imported_bindings
                .iter()
                .filter(|binding| !binding.is_type_only)
                .map(|binding| binding.local.as_str())
                .collect();
            let calls: FxHashMap<_, _> = flow.calls.iter()
                .filter(|call| !flow.tagged_templates.contains_key(&(call.offset, call.end)))
                .map(|call| ((call.offset, call.source_callee.as_str()), call))
                .collect();
            let callbacks: FxHashSet<_> = wrappers
                .into_iter()
                .filter_map(|(key, index)| {
                    let call = calls.get(&key)?;
                    if call.bound
                        || call.spread_at.is_some_and(|spread| spread <= index)
                        || !direct_import(&call.callee, &imports)
                    {
                        return None;
                    }
                    let ValueExpression::Callable(callback) = call.arguments.get(index)? else {
                        return None;
                    };
                    (!flow.generator_callables.contains(callback)).then_some(*callback)
                })
                .collect();
            for (site, api) in publishes(sites, &callbacks, &targets.publishers, &calls, &flow.deferred_calls) {
                findings.push(finding(&root, application, site, api));
            }
        }
    }
    findings.sort();
    Ok(findings)
}

fn target(site: &ResolvedCallSite) -> Option<Api<'_>> {
    if site.invocation != InvocationKind::Call || !site.alternatives.is_empty() {
        return None;
    }
    match &site.target {
        ResolvedCallTarget::ModuleExport {
            specifier,
            export_path,
            ..
        } => Some((specifier, export_path)),
        _ => None,
    }
}

type CallKey<'a> = (u32, &'a str);

fn site_key(site: &ResolvedCallSite) -> CallKey<'_> {
    (site.offset, site.source_callee.as_str())
}

fn direct_import(callee: &ValueExpression, imports: &FxHashSet<&str>) -> bool {
    let mut base = callee;
    while let ValueExpression::Member(receiver, _) = base {
        base = receiver;
    }
    matches!(
        base,
        ValueExpression::Named { name, scope: None | Some(0), .. }
            if imports.contains(name.as_str())
    )
}

fn publishes<'a>(
    sites: &'a [ResolvedCallSite],
    callbacks: &'a FxHashSet<CallableId>,
    publishers: &'a FxHashSet<Api<'_>>,
    calls: &'a FxHashMap<CallKey<'a>, &'a ValueCall>,
    deferred: &'a FxHashSet<u32>,
) -> impl Iterator<Item = (&'a ResolvedCallSite, Api<'a>)> {
    sites.iter().filter_map(move |site| {
        if deferred.contains(&site.offset)
            || !callbacks.contains(&site.caller_id?)
            || calls.get(&site_key(site))?.bound
        {
            return None;
        }
        let api = target(site)?;
        publishers.contains(&api).then_some((site, api))
    })
}

fn finding(root: &Path, application: &RuleDef, site: &ResolvedCallSite, api: Api<'_>) -> RuleFinding {
    let body = "Move this enqueue after the transaction commits. Return the job data from the callback and enqueue it after the transaction call resolves.";
    let message = match application.message.as_deref() {
        Some(prefix) => format!("{prefix}: {body}"),
        None => body.to_owned(),
    };
    RuleFinding {
        rule: RULE_ID.into(),
        file: relative_slash_path(root, &site.file),
        line: site.line as usize,
        message,
        import: Some(site.source_callee.clone()),
        target: Some(format!("{}#{}", api.0, api.1)),
        causes: Vec::new(),
    }
}

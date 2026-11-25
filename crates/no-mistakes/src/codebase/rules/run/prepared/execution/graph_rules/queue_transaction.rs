use super::*;
use crate::codebase::rules::queue_no_enqueue_in_transaction::{check, RULE_ID};

pub(super) fn collect(
    request: &GraphRuleRequest<'_>,
    session: Option<&crate::codebase::analysis_session::AnalysisSession>,
    findings: &mut Vec<RuleFinding>,
) -> Result<()> {
    if !request.config.rule_configured(RULE_ID)
        || (request.dependency_graph.is_none()
            && session.is_some_and(|session| !session.check_consumer_ready(RULE_ID)))
    {
        return Ok(());
    }
    let result = crate::perf_trace::rule("analysis.rules.queue-no-enqueue-in-transaction", || {
        check(
            request.root,
            request.config,
            request
                .dependency_graph
                .expect("queue transaction rule requires canonical graph"),
            request.shared,
        )
    });
    let output = match session {
        Some(session) => session.capture_check_result(RULE_ID, result)?,
        None => Some(result?),
    };
    findings.extend(output.into_iter().flatten());
    Ok(())
}

//! One title in full, from a provider's definition.

use super::card;
use super::fetch::{Asking, get};
use crate::calendar::now_ms;
use crate::definition::{DetailDef, ProviderDef};
use crate::expression::{Env, Row, matches, pattern, select};
use crate::model::{Detail, Kind};
use serde_json::Value;
use std::time::Duration;

const TTL: Duration = Duration::from_secs(10 * 60);

/// The title, or `None` when the service does not have it or its id is not one the service has.
pub async fn detail(
    def: &ProviderDef,
    kind: Kind,
    provider_id: &str,
) -> anyhow::Result<Option<Detail>> {
    let detail = &def.detail;
    if let Some(valid) = &detail.id_pattern
        && !pattern(valid)?.is_match(provider_id)
    {
        return Ok(None);
    }
    let mut asking = Asking::new(def, kind);
    asking.id = Some(provider_id);
    let request = asking
        .prepare(&detail.request)
        .ok_or_else(|| anyhow::anyhow!("the request cannot be filled in"))?;
    let Some(answer) = get(
        &def.id,
        kind,
        request,
        detail.check.as_ref(),
        TTL,
        &detail.root,
        true,
    )
    .await?
    else {
        return Ok(None);
    };

    // The lists that are asked for on their own. They are a nicety, so a failure leaves them
    // empty instead of failing the page.
    let mut separate = [None, None];
    for (slot, list) in separate
        .iter_mut()
        .zip([&detail.related, &detail.recommended])
    {
        let Some(list) = list else { continue };
        let Some(request) = &list.request else {
            continue;
        };
        let Some(request) = asking.prepare(request) else {
            continue;
        };
        *slot = get(
            &def.id,
            kind,
            request,
            list.check.as_ref(),
            TTL,
            &list.rows,
            false,
        )
        .await
        .ok()
        .flatten();
    }
    Ok(read(
        def,
        kind,
        &answer,
        [separate[0].as_deref(), separate[1].as_deref()],
        now_ms(),
    ))
}

/// The title out of its answer.
fn read(
    def: &ProviderDef,
    kind: Kind,
    answer: &Value,
    separate: [Option<&Value>; 2],
    now_ms: i64,
) -> Option<Detail> {
    let detail: &DetailDef = &def.detail;
    let title = select(answer, &detail.root).into_iter().next()?;
    if !title.is_object() {
        return None;
    }
    let env = Env {
        now_ms,
        active: &[],
    };
    let row = Row {
        value: title,
        parent: None,
        kind,
    };
    if detail
        .missing
        .iter()
        .any(|condition| matches(condition, row, &env))
    {
        return None;
    }
    card::detail(&def.card, detail, kind, title, separate, &env)
}

#[cfg(test)]
pub(super) fn read_for_test(
    def: &ProviderDef,
    kind: Kind,
    answer: &Value,
    separate: [Option<&Value>; 2],
) -> Option<Detail> {
    read(def, kind, answer, separate, 0)
}

use tokio::sync::mpsc;

use crate::handle::BrokerRequest;
use pneuma_engines::HeadlessEngine;

pub async fn run(
    mut rx: mpsc::UnboundedReceiver<BrokerRequest>,
    engine: Box<dyn HeadlessEngine>,
) {
    tracing::info!(target: "pneuma_broker", "service loop started");
    let mut next_page_id: u32 = 1;
    let mut engine_closed = false;

    while let Some(req) = rx.recv().await {
        match req {
            BrokerRequest::CreatePage { reply } => {
                let page_id = next_page_id;
                next_page_id = next_page_id.saturating_add(1);
                tracing::info!(target: "pneuma_broker", page_id, "CreatePage");
                let _ = reply.send(Ok(page_id));
            }
            // WEEK-7-ENTRY-POINT
            BrokerRequest::Navigate {
                page_id,
                url,
                opts_json,
                reply,
            } => {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    url = %url,
                    opts_len = opts_json.len(),
                    "Navigate"
                );
                let result = engine.navigate(&url, &opts_json).await;
                let _ = reply.send(result);
            }
            BrokerRequest::Evaluate {
                page_id,
                script,
                reply,
            } => {
                tracing::info!(
                    target: "pneuma_broker",
                    page_id,
                    script_len = script.len(),
                    "Evaluate"
                );
                let result = engine.evaluate(&script).await;
                let _ = reply.send(result);
            }
            BrokerRequest::Screenshot { page_id, reply } => {
                tracing::info!(target: "pneuma_broker", page_id, "Screenshot");
                let result = engine.screenshot().await;
                let _ = reply.send(result);
            }
            BrokerRequest::CloseBrowser { reply } => {
                tracing::info!(target: "pneuma_broker", "CloseBrowser");
                let result = engine.close().await;
                if result.is_ok() {
                    engine_closed = true;
                }
                let _ = reply.send(result);
            }
            BrokerRequest::Shutdown { reply } => {
                tracing::info!(target: "pneuma_broker", "Shutdown - exiting service loop");
                let result = engine.close().await;
                if result.is_ok() {
                    engine_closed = true;
                }
                let _ = reply.send(result);
                break;
            }
        }
    }

    if !engine_closed {
        if let Err(error) = engine.close().await {
            tracing::warn!(
                target: "pneuma_broker",
                error = %error,
                "engine close during service shutdown failed"
            );
        }
    }

    tracing::info!(target: "pneuma_broker", "service loop exited");
}

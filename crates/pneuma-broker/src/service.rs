use tokio::sync::mpsc;

use crate::handle::BrokerRequest;

pub async fn run(mut rx: mpsc::UnboundedReceiver<BrokerRequest>) {
    tracing::info!(target: "pneuma_broker", "service loop started");

    while let Some(req) = rx.recv().await {
        match req {
            BrokerRequest::CreatePage { reply } => {
                tracing::info!(target: "pneuma_broker", "CreatePage");
                let _ = reply.send(Ok(1));
            }
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
                    "Navigate - engine not yet wired"
                );
                let _ = reply.send(Ok(
                    r#"{"ok":true,"engine":"stub","migrated":false}"#.to_string()
                ));
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
                    "Evaluate - engine not yet wired"
                );
                let _ = reply.send(Ok("null".to_string()));
            }
            BrokerRequest::Screenshot { page_id, reply } => {
                tracing::info!(target: "pneuma_broker", page_id, "Screenshot - stub");
                let _ = reply.send(Ok(Vec::new()));
            }
            BrokerRequest::CloseBrowser { reply } => {
                tracing::info!(target: "pneuma_broker", "CloseBrowser");
                let _ = reply.send(Ok(()));
            }
            BrokerRequest::Shutdown { reply } => {
                tracing::info!(target: "pneuma_broker", "Shutdown - exiting service loop");
                let _ = reply.send(Ok(()));
                break;
            }
        }
    }

    tracing::info!(target: "pneuma_broker", "service loop exited");
}

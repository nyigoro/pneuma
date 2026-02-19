use anyhow::{anyhow, Result};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub enum BrokerRequest {
    CreatePage {
        reply: oneshot::Sender<Result<u32>>,
    },
    Navigate {
        page_id: u32,
        url: String,
        opts_json: String,
        reply: oneshot::Sender<Result<String>>,
    },
    Evaluate {
        page_id: u32,
        script: String,
        reply: oneshot::Sender<Result<String>>,
    },
    Screenshot {
        page_id: u32,
        reply: oneshot::Sender<Result<Vec<u8>>>,
    },
    CloseBrowser {
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<()>>,
    },
}

#[derive(Clone, Debug)]
pub struct BrokerHandle {
    tx: mpsc::UnboundedSender<BrokerRequest>,
}

impl BrokerHandle {
    pub fn new(tx: mpsc::UnboundedSender<BrokerRequest>) -> Self {
        Self { tx }
    }

    fn round_trip<T, F>(&self, build_request: F) -> Result<T>
    where
        F: FnOnce(oneshot::Sender<Result<T>>) -> BrokerRequest,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(build_request(reply_tx))
            .map_err(|_| anyhow!("broker request channel closed"))?;
        reply_rx
            .blocking_recv()
            .map_err(|_| anyhow!("broker reply channel closed"))?
    }

    pub fn create_page(&self) -> Result<u32> {
        self.round_trip(|reply| BrokerRequest::CreatePage { reply })
    }

    pub fn navigate(&self, page_id: u32, url: String, opts_json: String) -> Result<String> {
        self.round_trip(|reply| BrokerRequest::Navigate {
            page_id,
            url,
            opts_json,
            reply,
        })
    }

    pub fn evaluate(&self, page_id: u32, script: String) -> Result<String> {
        self.round_trip(|reply| BrokerRequest::Evaluate {
            page_id,
            script,
            reply,
        })
    }

    pub fn screenshot(&self, page_id: u32) -> Result<Vec<u8>> {
        self.round_trip(|reply| BrokerRequest::Screenshot { page_id, reply })
    }

    pub fn close_browser(&self) -> Result<()> {
        self.round_trip(|reply| BrokerRequest::CloseBrowser { reply })
    }

    pub fn shutdown(&self) -> Result<()> {
        self.round_trip(|reply| BrokerRequest::Shutdown { reply })
    }
}

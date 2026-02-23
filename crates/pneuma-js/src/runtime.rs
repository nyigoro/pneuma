use anyhow::Result;
use pneuma_broker::handle::BrokerHandle;

#[cfg(feature = "quickjs")]
use crate::ffi_bridge;

#[cfg(feature = "quickjs")]
use rquickjs::promise::MaybePromise;
#[cfg(feature = "quickjs")]
use rquickjs::Runtime as QjsRuntime;
#[cfg(feature = "quickjs")]
use rquickjs::Value as JsValue;
#[cfg(feature = "quickjs")]
use std::sync::mpsc::{sync_channel, SyncSender};
#[cfg(feature = "quickjs")]
use std::thread::JoinHandle;

#[cfg(feature = "quickjs")]
const GHOST_SHIM: &str = include_str!("shim/ghost_shim.js");
#[cfg(feature = "quickjs")]
const STEALTH_SHIM: &str = include_str!("shim/stealth.js");
#[cfg(feature = "quickjs")]
const MAX_DRAIN_ITERATIONS: u32 = 1000;

#[cfg(feature = "quickjs")]
enum RuntimeCommand {
    Execute {
        source: String,
        reply: SyncSender<Result<()>>,
    },
    Eval {
        expr: String,
        reply: SyncSender<Result<String>>,
    },
    Shutdown {
        reply: SyncSender<Result<()>>,
    },
}

pub struct Runtime {
    #[cfg(feature = "quickjs")]
    tx: SyncSender<RuntimeCommand>,
    #[cfg(feature = "quickjs")]
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn new(broker: BrokerHandle) -> Result<Self> {
        #[cfg(feature = "quickjs")]
        {
            let (cmd_tx, cmd_rx) = sync_channel::<RuntimeCommand>(0);
            let (init_tx, init_rx) = sync_channel::<Result<()>>(0);

            let thread = std::thread::Builder::new()
                .name("pneuma-quickjs".into())
                .spawn(move || {
                    let runtime = match QjsRuntime::new() {
                        Ok(runtime) => runtime,
                        Err(error) => {
                            let _ = init_tx.send(Err(error.into()));
                            return;
                        }
                    };
                    let context = match rquickjs::Context::full(&runtime) {
                        Ok(context) => context,
                        Err(error) => {
                            let _ = init_tx.send(Err(error.into()));
                            return;
                        }
                    };

                    let init_result = context
                        .with(|ctx| -> rquickjs::Result<()> {
                            ffi_bridge::register(ctx.clone(), broker)?;
                            ctx.eval::<(), _>(STEALTH_SHIM)?;
                            ctx.eval::<(), _>(GHOST_SHIM)?;
                            Ok(())
                        })
                        .map_err(anyhow::Error::from);
                    if let Err(error) = init_result {
                        let _ = init_tx.send(Err(error));
                        return;
                    }
                    if init_tx.send(Ok(())).is_err() {
                        return;
                    }

                    tracing::info!(target: "pneuma_js", "QuickJS thread ready");

                    // Drain pending microtasks with a hard cap to avoid hangs on malformed scripts.
                    fn drain_jobs(ctx: rquickjs::Ctx<'_>) -> Result<()> {
                        // NOTE: Use ctx.execute_pending_job() here, NOT runtime.execute_pending_job().
                        // We are already inside context.with(...) which holds a RefCell borrow on the
                        // runtime. Calling through the runtime handle would panic with BorrowMutError.
                        // The context-scoped method avoids the double-borrow while providing identical
                        // job-pump semantics.
                        for _ in 0..MAX_DRAIN_ITERATIONS {
                            if ctx.execute_pending_job() {
                                continue;
                            }
                            return Ok(());
                        }
                        anyhow::bail!(
                            "async expression did not resolve after {MAX_DRAIN_ITERATIONS} job drain iterations - possible infinite loop in script"
                        )
                    }

                    fn serialize_value<'js>(
                        ctx: rquickjs::Ctx<'js>,
                        value: JsValue<'js>,
                    ) -> Result<String> {
                        if value.is_undefined() {
                            return Ok("null".into());
                        }
                        match ctx.json_stringify(value).map_err(anyhow::Error::from)? {
                            Some(serialized) => Ok(serialized.to_string()?),
                            None => Ok("null".into()),
                        }
                    }

                    fn resolve_promise<'js>(
                        ctx: rquickjs::Ctx<'js>,
                        maybe: MaybePromise<'js>,
                    ) -> Result<String> {
                        if let Some(value) = maybe.result::<JsValue>() {
                            let value = value.map_err(anyhow::Error::from)?;
                            return serialize_value(ctx, value);
                        }

                        drain_jobs(ctx.clone())?;

                        match maybe.result::<JsValue>() {
                            Some(value) => {
                                let value = value.map_err(anyhow::Error::from)?;
                                serialize_value(ctx, value)
                            }
                            None => anyhow::bail!(
                                "Promise did not resolve: possible infinite loop or unresolvable microtask chain (cap: 1000 iterations)"
                            ),
                        }
                    }

                    while let Ok(command) = cmd_rx.recv() {
                        match command {
                            RuntimeCommand::Execute { source, reply } => {
                                let result = context.with(|ctx| -> Result<()> {
                                    ctx.eval::<(), _>(source.as_str())
                                        .map_err(anyhow::Error::from)?;
                                    drain_jobs(ctx.clone())?;
                                    Ok(())
                                });
                                let _ = reply.send(result);
                            }
                            RuntimeCommand::Eval { expr, reply } => {
                                let result = context.with(|ctx| -> Result<String> {
                                    let maybe: MaybePromise =
                                        ctx.eval(expr.as_str()).map_err(anyhow::Error::from)?;
                                    resolve_promise(ctx, maybe)
                                });
                                let _ = reply.send(result);
                            }
                            RuntimeCommand::Shutdown { reply } => {
                                tracing::info!(target: "pneuma_js", "QuickJS thread shutting down");
                                let _ = reply.send(Ok(()));
                                break;
                            }
                        }
                    }

                    tracing::info!(target: "pneuma_js", "QuickJS thread exited");
                })?;

            return match init_rx.recv() {
                Ok(Ok(())) => {
                    tracing::info!(target: "pneuma_js", "Runtime initialized");
                    Ok(Self {
                        tx: cmd_tx,
                        thread: Some(thread),
                    })
                }
                Ok(Err(error)) => {
                    let _ = thread.join();
                    Err(error)
                }
                Err(_) => {
                    let _ = thread.join();
                    Err(anyhow::anyhow!("QuickJS thread exited before signaling init"))
                }
            };
        }

        #[cfg(not(feature = "quickjs"))]
        {
            let _ = broker;
            Ok(Self {})
        }
    }

    pub fn backend_name(&self) -> &'static str {
        #[cfg(feature = "quickjs")]
        {
            "quickjs"
        }

        #[cfg(not(feature = "quickjs"))]
        {
            "stub"
        }
    }

    pub fn execute_script(&self, source: &str) -> Result<()> {
        #[cfg(feature = "quickjs")]
        {
            let (reply_tx, reply_rx) = sync_channel(0);
            self.tx
                .send(RuntimeCommand::Execute {
                    source: source.to_string(),
                    reply: reply_tx,
                })
                .map_err(|_| anyhow::anyhow!("QuickJS thread has exited"))?;
            return reply_rx
                .recv()
                .map_err(|_| anyhow::anyhow!("QuickJS thread dropped reply"))?;
        }

        #[cfg(not(feature = "quickjs"))]
        {
            let _ = source;
            anyhow::bail!("pneuma-js was built without `quickjs` support")
        }
    }

    pub fn eval_expression(&self, expression: &str) -> Result<String> {
        #[cfg(feature = "quickjs")]
        {
            let (reply_tx, reply_rx) = sync_channel(0);
            self.tx
                .send(RuntimeCommand::Eval {
                    expr: expression.to_string(),
                    reply: reply_tx,
                })
                .map_err(|_| anyhow::anyhow!("QuickJS thread has exited"))?;
            return reply_rx
                .recv()
                .map_err(|_| anyhow::anyhow!("QuickJS thread dropped reply"))?;
        }

        #[cfg(not(feature = "quickjs"))]
        {
            let _ = expression;
            anyhow::bail!("pneuma-js was built without `quickjs` support")
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        #[cfg(feature = "quickjs")]
        {
            let (reply_tx, reply_rx) = sync_channel(0);
            let _ = self.tx.send(RuntimeCommand::Shutdown { reply: reply_tx });
            let _ = reply_rx.recv();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

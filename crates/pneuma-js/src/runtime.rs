use anyhow::Result;

const GHOST_SHIM: &str = include_str!("shim/ghost_shim.js");

pub struct Runtime {
    #[cfg(feature = "quickjs")]
    _runtime: rquickjs::Runtime,
    #[cfg(feature = "quickjs")]
    context: rquickjs::Context,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        #[cfg(feature = "quickjs")]
        {
            let runtime = rquickjs::Runtime::new()?;
            let context = rquickjs::Context::full(&runtime)?;
            context.with(|ctx| ctx.eval::<(), _>(GHOST_SHIM))?;
            return Ok(Self {
                _runtime: runtime,
                context,
            });
        }

        #[cfg(not(feature = "quickjs"))]
        {
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
            self.context.with(|ctx| ctx.eval::<(), _>(source))?;
            return Ok(());
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
            let wrapped = format!(
                "(function() {{
                    let __pneuma_value = ({expression});
                    let __pneuma_json = JSON.stringify(__pneuma_value);
                    return __pneuma_json === undefined ? String(__pneuma_value) : __pneuma_json;
                }})()"
            );
            let rendered = self
                .context
                .with(|ctx| ctx.eval::<String, _>(wrapped.as_str()))?;
            return Ok(rendered);
        }

        #[cfg(not(feature = "quickjs"))]
        {
            let _ = expression;
            anyhow::bail!("pneuma-js was built without `quickjs` support")
        }
    }
}

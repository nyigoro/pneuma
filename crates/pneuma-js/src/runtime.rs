use anyhow::Result;

#[derive(Debug, Default)]
pub struct Runtime {
    backend: Backend,
}

#[derive(Debug, Default)]
enum Backend {
    #[default]
    Stub,
    #[cfg(feature = "quickjs")]
    QuickJs,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        #[cfg(feature = "quickjs")]
        {
            let _rt = rquickjs::Runtime::new()?;
            let _ctx = rquickjs::Context::full(&_rt)?;
            return Ok(Self {
                backend: Backend::QuickJs,
            });
        }

        #[cfg(not(feature = "quickjs"))]
        {
            Ok(Self {
                backend: Backend::Stub,
            })
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self.backend {
            Backend::Stub => "stub",
            #[cfg(feature = "quickjs")]
            Backend::QuickJs => "quickjs",
        }
    }
}

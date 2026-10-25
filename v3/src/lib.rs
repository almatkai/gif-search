mod domain;
use serde_json::Value;
use tezbar_extension_sdk::{BoxFuture, CoreResult, Extension, ExtensionContext};

pub struct GifSearch;
impl Extension for GifSearch {
    fn id(&self) -> &'static str {
        "com.tezbar.native.gif-search"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn commands(&self) -> &'static [&'static str] {
        domain::commands()
    }
    fn handle<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        command: &'a str,
        operation: &'a str,
        payload: &'a Value,
        state: &'a mut Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(domain::handle(ctx, command, operation, payload, state))
    }
}

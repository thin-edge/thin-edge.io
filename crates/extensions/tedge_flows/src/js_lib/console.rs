use crate::js_script::JsonValue;
use rquickjs::class::Trace;
use rquickjs::function::Rest;
use rquickjs::Ctx;
use rquickjs::JsLifetime;
use rquickjs::Result;
use rquickjs::Value;
use std::fmt::Write;

#[derive(Clone, Trace, JsLifetime)]
#[rquickjs::class(frozen)]
struct Console {}

pub fn init(ctx: &Ctx<'_>) {
    let console = Console {};
    let _ = ctx.globals().set("console", console);
}

impl Console {
    fn print(&self, _level: tracing::Level, values: Rest<Value<'_>>) -> Result<()> {
        let mut message = String::new();
        for (i, value) in values.0.into_iter().enumerate() {
            if i > 0 {
                let _ = write!(&mut message, ", ");
            }
            let _ = write!(&mut message, "{}", JsonValue::display(value));
        }
        eprintln!("JavaScript.Console: {message}");
        Ok(())
    }
}

#[rquickjs::methods]
impl Console {
    fn debug(&self, values: Rest<Value<'_>>) -> Result<()> {
        self.print(tracing::Level::DEBUG, values)
    }

    fn log(&self, values: Rest<Value<'_>>) -> Result<()> {
        self.print(tracing::Level::INFO, values)
    }

    fn warn(&self, values: Rest<Value<'_>>) -> Result<()> {
        self.print(tracing::Level::WARN, values)
    }

    fn error(&self, values: Rest<Value<'_>>) -> Result<()> {
        self.print(tracing::Level::ERROR, values)
    }
}

use crate::js_value::JsonValue;
use rquickjs::class::Trace;
use rquickjs::Ctx;
use rquickjs::IntoJs;
use rquickjs::JsLifetime;
use rquickjs::Object;
use rquickjs::Result;
use rquickjs::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Default, JsLifetime)]
pub struct FlowContextHandle {
    handle: Arc<Mutex<LayeredKVStore>>,
}

#[derive(Default)]
struct LayeredKVStore {
    global: BTreeMap<String, JsonValue>,
    scoped: HashMap<FlowContext, BTreeMap<String, JsonValue>>,
}

impl FlowContextHandle {
    pub fn init(&self, ctx: &Ctx<'_>) {
        self.store_as_userdata(ctx)
    }

    pub fn js_context<'js>(
        ctx: &Ctx<'js>,
        flow_name: &str,
        script_name: &str,
        config: &JsonValue,
    ) -> Result<Value<'js>> {
        let context = Object::new(ctx.clone())?;

        context.set("mapper", FlowContext::Mapper)?;
        context.set("flow", FlowContext::flow(flow_name))?;
        context.set("script", FlowContext::script(script_name))?;
        context.set("config", config)?;

        context.into_js(ctx)
    }

    pub fn get(&self, context: &FlowContext, key: &str) -> JsonValue {
        self.handle.lock().unwrap().get(context, key)
    }

    pub fn insert(&self, context: &FlowContext, key: &str, value: impl Into<JsonValue>) {
        let mut data = self.handle.lock().unwrap();
        data.insert(context, key, value);
    }

    pub fn keys(&self, context: &FlowContext) -> Vec<String> {
        self.handle.lock().unwrap().keys(context)
    }

    pub fn remove(&self, context: &FlowContext, key: &str) {
        let mut data = self.handle.lock().unwrap();
        data.remove(context, key);
    }

    fn store_as_userdata(&self, ctx: &Ctx<'_>) {
        let _ = ctx.store_userdata(self.clone());
    }

    fn get_from_userdata(ctx: &Ctx<'_>) -> Self {
        match ctx.userdata::<Self>() {
            None => {
                let store = FlowContextHandle::default();
                store.store_as_userdata(ctx);
                store
            }
            Some(userdata) => userdata.deref().clone(),
        }
    }
}

impl LayeredKVStore {
    fn context(&self, context: &FlowContext) -> Option<&BTreeMap<String, JsonValue>> {
        if context.is_global() {
            Some(&self.global)
        } else {
            self.scoped.get(context)
        }
    }

    fn context_mut(&mut self, context: &FlowContext) -> Option<&mut BTreeMap<String, JsonValue>> {
        if context.is_global() {
            Some(&mut self.global)
        } else {
            self.scoped.get_mut(context)
        }
    }

    fn entry(&mut self, context: &FlowContext) -> &mut BTreeMap<String, JsonValue> {
        if context.is_global() {
            &mut self.global
        } else {
            self.scoped.entry(context.clone()).or_default()
        }
    }

    fn get(&self, context: &FlowContext, key: &str) -> JsonValue {
        match self.context(context) {
            None => JsonValue::Null,
            Some(map) => map.get(key).cloned().unwrap_or(JsonValue::Null),
        }
    }

    fn insert(&mut self, context: &FlowContext, key: &str, value: impl Into<JsonValue>) {
        match value.into() {
            JsonValue::Null => self.remove(context, key),
            value => {
                let map = self.entry(context);
                map.insert(key.to_owned(), value);
            }
        }
    }

    fn keys(&self, context: &FlowContext) -> Vec<String> {
        match self.context(context) {
            None => vec![],
            Some(map) => map.keys().cloned().collect(),
        }
    }

    pub fn remove(&mut self, context: &FlowContext, key: &str) {
        if let Some(map) = self.context_mut(context) {
            map.remove(key);
        }
    }
}

#[derive(Clone, Trace, JsLifetime, Hash, Eq, PartialEq)]
#[rquickjs::class(frozen)]
pub(crate) enum FlowContext {
    Mapper,
    Flow(String),
    Script(String),
}

impl FlowContext {
    pub(crate) fn flow(name: &str) -> Self {
        FlowContext::Flow(name.to_owned())
    }

    pub(crate) fn script(name: &str) -> Self {
        FlowContext::Script(name.to_owned())
    }

    fn is_global(&self) -> bool {
        self == &FlowContext::Mapper
    }
}

#[rquickjs::methods]
impl<'js> FlowContext {
    fn get(&self, ctx: Ctx<'js>, key: String) -> Result<JsonValue> {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        Ok(data.get(self, &key))
    }

    fn set(&self, ctx: Ctx<'js>, key: String, value: JsonValue) {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        data.insert(self, &key, value)
    }

    fn remove(&self, ctx: Ctx<'js>, key: String) {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        data.remove(self, &key)
    }

    fn keys(&self, ctx: Ctx<'js>) -> Vec<String> {
        let data = FlowContextHandle::get_from_userdata(&ctx);
        data.keys(self)
    }
}

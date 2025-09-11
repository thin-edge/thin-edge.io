use crate::flow::DateTime;
use crate::flow::FlowError;
use crate::flow::Message;
use anyhow::Context;
use rquickjs::Ctx;
use rquickjs::FromJs;
use rquickjs::IntoJs;
use rquickjs::Value;

#[derive(Clone, Debug)]
pub struct JsonValue(pub serde_json::Value);

impl Default for JsonValue {
    fn default() -> Self {
        JsonValue(serde_json::Value::Object(Default::default()))
    }
}

impl From<Message> for JsonValue {
    fn from(value: Message) -> Self {
        JsonValue(value.json())
    }
}

impl From<DateTime> for JsonValue {
    fn from(value: DateTime) -> Self {
        JsonValue(value.json())
    }
}

impl TryFrom<serde_json::Value> for Message {
    type Error = FlowError;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        let message = serde_json::from_value(value)
            .with_context(|| "Couldn't extract message payload and topic")?;
        Ok(message)
    }
}

impl TryFrom<JsonValue> for Message {
    type Error = FlowError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        Message::try_from(value.0)
    }
}

impl TryFrom<JsonValue> for Vec<Message> {
    type Error = FlowError;

    fn try_from(value: JsonValue) -> Result<Self, Self::Error> {
        match value.0 {
            serde_json::Value::Array(array) => array.into_iter().map(Message::try_from).collect(),
            serde_json::Value::Object(map) => {
                Message::try_from(serde_json::Value::Object(map)).map(|message| vec![message])
            }
            serde_json::Value::Null => Ok(vec![]),
            _ => Err(
                anyhow::anyhow!("Flow scripts are expected to return an array of messages").into(),
            ),
        }
    }
}

struct JsonValueRef<'a>(&'a serde_json::Value);

impl<'js> IntoJs<'js> for JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        JsonValueRef(&self.0).into_js(ctx)
    }
}

impl<'js> IntoJs<'js> for &JsonValue {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        JsonValueRef(&self.0).into_js(ctx)
    }
}

impl<'js> IntoJs<'js> for JsonValueRef<'_> {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self.0 {
            serde_json::Value::Null => Ok(Value::new_null(ctx.clone())),
            serde_json::Value::Bool(value) => Ok(Value::new_bool(ctx.clone(), *value)),
            serde_json::Value::Number(value) => {
                if let Some(n) = value.as_i64() {
                    if let Ok(n) = i32::try_from(n) {
                        return Ok(Value::new_int(ctx.clone(), n));
                    }
                }
                if let Some(f) = value.as_f64() {
                    return Ok(Value::new_float(ctx.clone(), f));
                }
                let nan = rquickjs::String::from_str(ctx.clone(), "NaN")?;
                Ok(nan.into_value())
            }
            serde_json::Value::String(value) => {
                let string = rquickjs::String::from_str(ctx.clone(), value)?;
                Ok(string.into_value())
            }
            serde_json::Value::Array(values) => {
                let array = rquickjs::Array::new(ctx.clone())?;
                for (i, value) in values.iter().enumerate() {
                    array.set(i, JsonValueRef(value))?;
                }
                Ok(array.into_value())
            }
            serde_json::Value::Object(values) => {
                let object = rquickjs::Object::new(ctx.clone())?;
                for (key, value) in values.into_iter() {
                    object.set(key, JsonValueRef(value))?;
                }
                Ok(object.into_value())
            }
        }
    }
}

impl<'js> FromJs<'js> for JsonValue {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        JsonValue::from_js_value(value)
    }
}

impl JsonValue {
    fn from_js_value(value: Value<'_>) -> rquickjs::Result<Self> {
        if let Some(promise) = value.as_promise() {
            // Beware checking the value is a promise must be done first
            // as a promise can also be used as an object
            return promise.finish();
        }
        if let Some(b) = value.as_bool() {
            return Ok(JsonValue(serde_json::Value::Bool(b)));
        }
        if let Some(n) = value.as_int() {
            return Ok(JsonValue(serde_json::Value::Number(n.into())));
        }
        if let Some(n) = value.as_float() {
            let js_n = serde_json::Number::from_f64(n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null);
            return Ok(JsonValue(js_n));
        }
        if let Some(string) = value.as_string() {
            return Ok(JsonValue(serde_json::Value::String(string.to_string()?)));
        }
        if let Some(array) = value.as_array() {
            let array: rquickjs::Result<Vec<JsonValue>> = array.iter().collect();
            let array = array?.into_iter().map(|v| v.0).collect();
            return Ok(JsonValue(serde_json::Value::Array(array)));
        }
        if let Some(object) = value.as_object() {
            let mut js_object = serde_json::Map::new();
            for key in object.keys::<String>().flatten() {
                if let Ok(JsonValue(v)) = object.get(&key) {
                    js_object.insert(key, v.clone());
                }
            }
            return Ok(JsonValue(serde_json::Value::Object(js_object)));
        }

        Ok(JsonValue(serde_json::Value::Null))
    }

    pub(crate) fn display(value: Value<'_>) -> String {
        let json = JsonValue::from_js_value(value).unwrap_or_default();
        serde_json::to_string_pretty(&json.0).unwrap()
    }
}

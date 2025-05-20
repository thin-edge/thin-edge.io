/// Transform:
///
/// ```
/// [te/device/main///m/example] {
///     "time": "2020-10-15T05:30:47+00:00",
///     "temperature": 25,
///     "location": {
///         "latitude": 32.54,
///         "longitude": -117.67,
///         "altitude": 98.6
///     },
///     "pressure": 98
/// }
/// ```
///
/// into
///
/// ```
/// [c8y/measurement/measurements/create] {
///     "time": "2020-10-15T05:30:47Z",
///     "type": "example",
///     "temperature": {
///         "temperature": {
///             "value": 25
///         }
///     },
///     "location": {
///         "latitude": {
///             "value": 32.54
///         },
///         "longitude": {
///             "value": -117.67
///         },
///         "altitude": {
///             "value": 98.6
///         }
///     },
///     "pressure": {
///         "pressure": {
///             "value": 98
///         }
///     }
/// }
/// ```
export function process(t, message, config) {
  let topic_parts = message.topic.split( '/')
  let type = topic_parts[6]
  let payload = JSON.parse(message.payload)

  let c8y_msg = {
    type: type
  }

  let meta = (config || {})[`${message.topic}/meta`] || {}

  for (let [k, v] of Object.entries(payload)) {
    let k_meta = (meta || {})[k] || {}
    if (k === "time") {
      let fragment = { time: v }
      Object.assign(c8y_msg, fragment)
    }
    else if (typeof(v) === "number") {
      if (Object.keys(k_meta).length>0) {
        v = { value: v, ...k_meta }
      }
      let fragment = { [k]: { [k]: v } }
      Object.assign(c8y_msg, fragment)
    } else for (let [sub_k, sub_v] of Object.entries(v)) {
      let sub_k_meta = k_meta[sub_k]
      if (typeof(sub_v) === "number") {
        if (sub_k_meta) {
          sub_v = { value: sub_v, ...sub_k_meta }
        }
        let fragment = { [k]: { [sub_k]: sub_v } }
        Object.assign(c8y_msg, fragment)
      }
    }
  }

  return [{
    topic: "c8y/measurement/measurements/create",
    payload: JSON.stringify(c8y_msg)
  }]
}

/// Update the config with measurement metadata.
///
/// These metadata are expected to have the same shape of the actual values.
///
/// ```
/// [te/device/main///m/example/meta] { "temperature": { "unit": "°C" }}
/// ```
///
/// and:
/// ```
/// [te/device/main///m/example] { "temperature": { "unit": 23 }}
/// ```
///
/// will be merged by the process function into:
/// ```
/// [c8y/measurement/measurements/create] {
///   "type": "example",
///   "temperature": {
///     "temperature": {
///       "value": 23,
///       "unit": "°C"
///     }
///   }
/// }
/// ```
export function update_config(message, config) {
  let type = message.topic
  let metadata = JSON.parse(message.payload)

  let fragment = {
    [type]: metadata
  }
  if (!config) {
    config = {}
  }
  Object.assign(config, fragment)

  return config
}

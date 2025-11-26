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
export function onMessage(message, config, context) {
  let topic_parts = message.topic.split( '/')
  let type = topic_parts[6] || "ThinEdgeMeasurement"
  let payload = JSON.parse(message.payload)

  let c8y_msg = {
    type: type
  }

  let meta = context.get(`${message.topic}/meta`) || {}

  for (let [k, v] of Object.entries(payload)) {
    let k_meta = meta[k] || {}
    if (k === "time") {
      let t = v
      if (typeof(v) === "number") {
        t = (new Date(v * 1000)).toISOString()
      }
      let fragment = { time: t }
      Object.assign(c8y_msg, fragment)
    }
    else if (typeof(v) === "number") {
      if (Object.keys(k_meta).length>0) {
        v = { value: v, ...k_meta }
      } else {
        v = { value: v }
      }
      let fragment = { [k]: { [k]: v } }
      Object.assign(c8y_msg, fragment)
    } else {
      let fragment = {}
      for (let [sub_k, sub_v] of Object.entries(v)) {
        let sub_k_meta = k_meta[sub_k]
        if (typeof(sub_v) === "number") {
          if (sub_k_meta) {
            sub_v = { value: sub_v, ...sub_k_meta }
          } else {
            sub_v = { value: sub_v }
          }
          let sub_fragment = { [sub_k]: sub_v }
          Object.assign(fragment, sub_fragment)
        }
      }
      Object.assign(c8y_msg, { [k]: fragment})
    }
  }

  return [{
    topic: "c8y/measurement/measurements/create",
    payload: JSON.stringify(c8y_msg)
  }]
}

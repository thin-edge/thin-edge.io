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
export function process(t,message) {
  let topic_parts = message.topic.split( '/')
  let type = topic_parts[6]
  let payload = JSON.parse(message.payload)

  let c8y_msg = {
    type: type
  }

  for (let [k, v] of Object.entries(payload)) {
    if (k === "time") {
      let fragment = { time: v }
      Object.assign(c8y_msg, fragment)

    }
    else if (typeof(v) === "number") {
      let fragment = { [k]: { [k]: v } }
      Object.assign(c8y_msg, fragment)
    } else for (let [sub_k, sub_v] of Object.entries(v)) {
      if (typeof(sub_v) === "number") {
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

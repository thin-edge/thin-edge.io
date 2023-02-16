# Custom Fragments

**Custom-Fragments** are snippets of meta-information, provided by processes running on the **main-device** or any **child-device**;
that meta-information can be e.g. the serial-number of the device, kinds of sensors the device provides, or signal strength of the device's LTE/3G connection.

* all **custom-fragments** are provided to **thin-edge** via MQTT topics
* the cloud mapper sends all **custom-fragments** to the cloud's corresponding **device-twins**
* processes running on the **main-device** and **child-devices** can consume and use the provided **custom-fragments** from MQTT (when not restricted for any security reason)

## Custom-Fragments on MQTT

All **custom-fragments** are provided and consumed via MQTT.


#### Topics

```cjson
tedge/inventory/<device-id>/<identifider>
```

|Reference     | Description|
| ------------ | ---------- |
|`device-id`   | if the custom-fragment is meant for a **child-device**, it is the corresponding **child-id**; or `main` if it is meant for the **main-device**|
|`identifider` | a string that uniquely identifies the custom-fragment in context of the device; to avoid any clash with other's fragments (e.g. thin-edge, other 3rd party components or the cloud-provider ones), it is a good practice to use some custom specific prefix|

#### Payload

The payload of the MQTT message carries the **custom-fragment's** information in JSON format.
It can be a simple scalar value, as e.g. `"LTE"`, or a nested structure, as e.g.
```json
{
      "version": "10",
      "arch": "arm64"
}
```

#### Behaviour

All **custom-fragments** shall be published as MQTT retain message to the broker, to keep them available for every subscriber at any time.
To remove a **custom-fragment** from the MQTT broker, an empty retain message shall be published to the corresponding topic.


### Example

The figure below shows exemplary command lines to add/remove fragments to devices (on the left side);
as well as command lines to query for those fragments (on the right side).


```bash
#┌─────────────────────────────────┬──────────────────────────────────────────────────┐
#│  ↓ (adding/removing fragments) ↓  │          ↓ (querying for fragments) ↓          │

# add fragment 'mobile_technology' to the main-device
$ tedge mqtt pub -r 'tedge/inventory/main/mobile_technology' 'LTE'

                                    # query all fragments of the main-device
                                    $ tedge mqtt sub tedge/inventory/main/#
                                    [.../mobile_technology] 'LTE'

# add fragment 'device_firmware' to the main-device
$ tedge mqtt pub -r 'tedge/inventory/main/device_firmware' '{
      "version": "10",
      "arch": "arm64"
   }'

                                    # query all fragments of the main-device
                                    $ tedge mqtt sub tedge/inventory/main/#
                                    [.../mobile_technology] 'LTE'
                                    [.../device_firmware]   '{
                                                              "version": "10",
                                                              "arch": "arm64"
                                                           }'


# remove fragment 'device_firmware' from the main-device
$ tedge mqtt pub -r 'tedge/inventory/main/device_firmware' ''

                                    # query all fragments of the main-device
                                    $ tedge mqtt sub tedge/inventory/main/#
                                    [.../mobile_technology] 'LTE'


# add fragment 'temperature' to the main-device
$ tedge mqtt pub -r 'tedge/inventory/main/temperature'   '{ "num_of_sensors": 1 }'

# add fragment 'temperature' to the child-device "child1"
$ tedge mqtt pub -r 'tedge/inventory/child1/temperature' '{ "num_of_sensors": 3 }'


                                    # query fragments 'temperature' of all devices
                                    $ tedge mqtt sub tedge/inventory/+/temperature
                                    [.../main/temperature]   '{ "num_of_sensors": 1 }'
                                    [.../child1/temperature] '{ "num_of_sensors": 3 }'

#└─────────────────────────────────┴──────────────────────────────────────────────────┘
```

TODO: Append to the flow some hints about what happens with the Mapper (e.g. when a fragment is sent to the cloud).

TODO: Consider/evaluate chance/benefit/drawback for partial update of a fragment.

TODO: Consider/evaluate how to address (C8Y) child-additions.

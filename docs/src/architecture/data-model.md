
# thin-edge Data Model

The **thin-edge data model** is used to represent all device-related information.

TODO: add more information what the data model is about, and mention/link to the domain model

## Inventory
**thin-edge** holds and manages an **inventory**.
That **inventory** is the communication backbone for **thin-edge** and all devices and components working with **thin-edge**.
It is the place that holds information about all **devices** (i.E. the **main-device** and all **child-devices**).
  * All processes on all devices can _put_ information into the **inventory**.
  * And all procssess on all devices can _query_ information from the **inventory**.
  * The inventory can be used to anounce and discover **telemetry data**.
    * Processes (on any device) that provide **telemetry data**, e.g. **measurements** or **alarms**, can put descriptions of those into the **inventory**.
    * Other processes (on any device) that react on a specific **alarm type** or consume a specific **measurement type** can query the **inventory** for those.
      Based on the query result the processes can subscribe to corresponding **alarm** / **measurement** MQTT topics to consume occurring alarm-triggers / measurement samples.
  * Also **plugins** can use the inventory, to discover **child-devices** or containers that intend to make use of their functionality.
    * Proccesses (on any device) that e.g. provide log files can put a list of those log files into the **inventory**.
    * The log-management plugin can query the **inventory** for those log file lists (from any device).
      Based on the query result it can operate with corresponding devices to provide those log files to the cloud.

### Format and content

The **inventory** stores for each device a _data object_ with certain _fields_.

The figure below illustrats the **inventory** and its _device objects_.

![thin-edge Inventory](images/inventory.svg)

* The **thin-edge Device** object represents the **main device**, that runs **thin-edge** and manages that **inventory**.
  * The fields `name` and `type` contain the _device-name_ and _device-type_ visible in the cloud.
  * The field `telemetry_descriptor` contains descriptions of all **measurements**, **setpoints**, **events** and **alarms** the device provides.
    Details about the `telemetry_descriptor` are explained in section [Telemtry Descriptor](#telemtry-descriptor) below.
  * The field `plugin_descriptor` contains descriptions of all **plugins** the **device** intends to make use of
    (e.g. _Software Management_, _Configuration Management_, _Log file Management_, or any custom specific **plugin**).
    Details about the `plugin_descriptor` are explained in section [Plugin Descriptor](#plugin-descriptor) below.
* A **Child-Device** object could be exist more than once in the inventory. 
  Each **Child-Device** object represents an _external device_ (e.g. sensor, actuator, PLC, any other kind of device) that is connected to the thin-edge device.
  * Each **child-device** object is assocoiated with a separate individual device in the cloud. 
  * Similar to the **thin-edge Device** object, each **child-device** object has the fields `name`, `type`, `telemetry_descriptor` and `plugin_descriptor`.
    In addition, each **child-device** object has a field `childid`, that contains a unique ID to address that child-device.
  * NOTE: Not just _external devices_, but also processes running on the thin-edge device itself, can be represented with a **child-device** object in the **inventory** - to treat them as __logical child-devices__.

### Telemtry Descriptor

The `telemetry_descriptor` is part of an **inventory's** _device object_ and contains descriptions for all **measurements**, **setpoints**, **events** and **alarms** the device provides.
For each kind of telemtry data the `telemetry_descriptor` holds an individual `struct`:

<!-- using below 'javascript' syntax-highlighter instead of 'json', since with JSON comments look really terrible -->
```javascript
   "telemetry_descriptor": {
      "measurements": [ /* ... specific structs for measurements ...   */ ],
      "setpoints":    [ /* ... specific structs for setpoints ...      */ ],
      "events":       [ /* ... specific structs for events ...         */ ],
      "alarms":       [ /* ... specific structs for alarms ...         */ ],
   }
```

Each struct `measurements`, `setpoints`, `events`, `alarms` has it's individual set of fields, just as needed to describe the coresponding kind of telemetry data.
Next sections describe those structures.

#### Struct `measurements`
```javascript
   "measurements": {
        /* 1st mesasurement */
        "<type_name>": {
            "num_values": /* <number of values> */,
            "units": ["<unit of 1st value>", "<unit of 2nd value>", ...]
        },

        /* next mesasurement */
        "<type_name2>": {
            /* ... */
        }
    }
 ```
  * Where
    * `type_name`, is a reference string, unique in scope of the given device object
    * `num-values`, number of values the measurement's **samples** carry
    * `units`, optional, list of units strings per values of the measurement's **samples**;
      that list is an ordered list of `units`, where the order must match the order of each **sample's** _value list_
    * TODO: field that describes values missing (that what is as comment in example above)
    * TODO: add a brief introduction about **samples**


Example:
```javascript
   /* Example, contains two measurements */
   "measurements": {
        "weather_station": {
            "num_values": 2,
            "units": ["%", "celsius"] // humidity and temperature
        },
        "power_meter": {
            "num_values": 3,
            "units": ["V", "V", "V"] // voltage of phase 1, 2, 3
        }
    }
 ```

#### Struct `setpoints`
```javascript
   "setpoints": {
        /* 1st setpoint */
        "<type_name>": {
            "num_values": /* <number of values> */
        },

        /* next setpoint */
        "<type_name2>": {
            /* ... */
        }

    }
````
  * Where
    * `type_name`, is a reference string, unique in scope of the given device object
    * `num-values`, number of values the setpoints carries
    * TODO: field that describes values missing (that what is as comment in example above)

Example:
```javascript
   /* Example, contains two setpoints */
   "setpoints": {
        "temperature_limits": {
            "num_values": 2 // set-points for a lower limit and a higher limit
        },
        "relay_array": {
            "num_values": 8 // 8 relays in series
        }
    }
 ```

#### Struct `events`
```javascript
   "events": {
        /* 1st event */
        "<type_name>": {
            /* beyond the type-name no more information available for events */
        },

        /* next event */
        "<type_name2>": {
            /* ... */
        }

    }
```

Example:
```javascript
   /* Example, contains two events */
   "events": {
        "door_opened": {
            /* no more information for events available */
        },
        "service_completed": {
            /* no more information for events available */
        }
    }
 ```
  * Where
    * `type_name`, is a reference string, unique in scope of the given device object

#### Struct `alarms`
```javascript
   "alarms": {
        /* 1st alarm */
        "<type_name>": {
            /* beyond the type-name no more information available for alarms */
        },

        /* next alarm */
        "<type_name2>": {
            /* ... */
        }
    }
````

Example:
```javascript
   /* Example, contains two alarms */
   "alarms": {
        "temperature_high": { // when higher temperature limit exceeded
            /* no more information for alarms available */
        },
        "temperature_low": { // when low temperature limit underrun
            /* no more information for alarms available */
        }
    }
 ```
  * Where
    * `type_name`, is a reference string, unique in scope of the given device object

### Plugin Descriptor

The `plugin_descriptor` is part of an **inventory's** _device object_.

The `plugin_descriptor` lists all **plugins** the **device** intends to connect. For each **plugin**, the `plugin_descriptor` contains the **plugin's** `plugin_identifier` and a **plugin's** specific `struct`.

<!-- using below 'javascript' syntax-highlighter instead of 'json', since with JSON comments look really terrible -->
```javascript
   "plugin_descriptor": {
      "<plugin_identifier 1>": { /* ... struct for plugin 1 ...   */ },
      "<plugin_identifier 2>": { /* ... struct for plugin 2 ...   */ },
      /* ... */
   }
```
  * Where
    * `plugin_identifier`, is a unique string referencing the plugin (e.g. `tedge_config` for the _c8y_configuration_plugin_)
    * the assigned `struct` is specific to the plugin referenced with the `plugin_identifier`

Example:
```javascript
   "plugin_descriptor": {
      "tedge_software":    { /* ... struct for thin-edge software management ... */ },
      "tedge_config":      { /* ... struct for thin-edge config management ...   */ },
      "tedge_log":         { /* ... struct for thin-edge log management ...      */ },
      "custom_plugin_foo": { /* ... struct for some custom specific plugin ...   */ },
      "custom_plugin_bar": { /* ... struct for another custom specific plugin .. */ },
   }
```

Each **plugin** defines it's own `struct` with individual set of fields, to contain all information the **plugin** needs to operate.

Each custom specific **plugin** has a unique `plugin_identifier` and `struct`, defined by the **plugin's** developer. For all **plugins** shipped with **thin-edge** `plugin_identifiers` and `structs` are defined as below.

#### Software Management
Plugin-Identifier: `tedge_software`
```javascript
TODO
```

#### Configuration Management
Plugin-Identifier: `tedge_config`
```javascript
   "tedge_config": {
      "files": [
         /* 1st configuration file */
         { "path": "<path to file>", "type": "<type_name>" },

         /* 2nd configuration file */
         { "path": "<path to file 2>", "type": "<type_name2>" },

         /* next configuration file */
         { /* ... */ },
      ]
   }
```
  * Where
    * `path`, is a file path; or a key path in some registry of the **device** or any name that makes sense for the corresponding **device**
    * `type_name`, is a reference string, used to name the configuration file on the cloud;
    * TODO: add fields user, group and mode (`user = "mosquitto", group = "mosquitto", mode = 0o644`)

Example:
```javascript
   "tedge_config": {
      "files": [
         { "path": "/etc/tedge/tedge.toml", "type": "tedge.toml" },
         { "path": "/etc/tedge/mosquitto-conf/c8y-bridge.conf" },
         { "path": "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf" },
         { "path": "/etc/mosquitto/mosquitto.conf", type = "mosquitto", user = "mosquitto", group = "mosquitto", mode = 0o644 }
      ]
   }
```

#### Log Management
Plugin-Identifier: `tedge_log`
```javascript
TODO
```

## Inventory API

TODO: That section is a rough draft version of the inventory API documentation.
The concept need to be detailed and explanation need to be improved significantly.

The **inventory** is available on the MQTT bus under the topic `tedge/inventory`.
* Each **inventorie's** _device object_ has it own topic: `/tedge/inventory/<device_id>`
  * The `device_id` is the _child-id_ of the **child-device** object, or `main` for the **main-device** object.
* All **telemetry data** descriptions are under the **device's** topic, followed by `telemetry_descriptor/<kind of telemetry data>/`m and the **telemetry** items `type_name`:
  * measurements:<br/>
    `tedge/inventory/<device_id>/telemetry_descriptor/measurements/<type_name>`<br/>
    Example for a measurement with _type_name_ "temperature":<br/>
    `tedge/inventory/main/telemetry_descriptor/measurements/temperature`
  * setpoints:<br/>
    `tedge/inventory/<device_id>/telemetry_descriptor/setpoint/<type_name>`
  * events:<br/>
    `tedge/inventory/<device_id>/telemetry_descriptor/event/<type_name>`
  * alarms:<br/>
    `tedge/inventory/<device_id>/telemetry_descriptor/alarm/<type_name>`
* All **plugin** descriptions are under the **device's** topic, followed by `/plugin_descriptor/` and the plugin's `plugin_identifier`:
  * `tedge/inventory/<device_id>/plugin_descriptor/<plugin_identifier>`<br/>
    Example for **Configuration Management**:<br/>
    `tedge/inventory/main/plugin_descriptor/tedeg_config`
* All messages on `tedge/inventory` and below are published as **retain messages**, since inventory data shall be kept at that place
  and shall immediately supplied to a subscriber at any time.

## Example Use-Cases

The examples in that section demonstrate common use-cases the **inventory** is used for.

All those examples make use of the `tedge` CLI MQTT client.
That way those examples are easily reproducible, and can be even tried and modified on the command line.

### Use-Case 1: Announce **measurements** to the **inventory**

**Description:**
Processes running on the **main-device** and **child-devices** that produce measurement values, can once announce those kinds of measurements to the **inventory**. That way other processes and the cloud-mapper can be aware of all available measurements.

**Examples:**
```bash
# announce measurement "machinery_temperature" for main-device
tedge mqtt pub -r \
  'tedge/inventory/main/telemetry_descriptor/measurements/machinery_temperature' \
  '
    {
      "num_values": 1,
      "units": ["celsius"]
    }
  '
```
```bash
# announce measurement "power_meter" for main-device
tedge mqtt pub -r \
  'tedge/inventory/main/telemetry_descriptor/measurements/power_meter' \
  '
    {
      "num_values": 6,
      /* ordered list of voltage and current of phase 1, 2, 3 */
      "units": ["V", "V", "V", "A", "A", "A"]
    }
  '
```
```bash
# announce measurement "machinery_temperature" for child-device "child1"
tedge mqtt pub -r \
  'tedge/inventory/child1/telemetry_descriptor/measurements/machinery_temperature' \
  '
    {
      "num_values": 1,
      "units": ["celsius"]
    }
  '
```


### Use-Case 2: Discover **measurements** in the **inventory**

**Description:**
Processes running on the **main-device** and **child-devices** that intend to consume specific measurements, can once discover those in the **inventory**. That way the **domain application**, or any 3rd party application (e.g. APAMA analytics) can use the information from **inventory** to learn which kinds of **measurements** are available.

NOTE: Examples below assume announcements of _use-case 1_ above had happened before.


**Example 1:**
```bash
# discovery all measurements "machinery_temperature"  provided by any device
tedge mqtt sub \
  'tedge/inventory/+/telemetry_descriptor/measurements/machinery_temperature'
```
```bash
# Output:
[tedge/inventory/main/telemetry_descriptor/measurements/machinery_temperature]
    {
      "num_values": 1,
      "units": ["celsius"]
    }

[tedge/inventory/child1/telemetry_descriptor/measurements/machinery_temperature]
    {
      "num_values": 1,
      "units": ["celsius"]
    }
```

**Example 2:**
```bash
# discovery all measurements the main-device provides
tedge mqtt sub \
  'tedge/inventory/main/telemetry_descriptor/measurements/#'
```
```bash
# Output:
[tedge/inventory/main/telemetry_descriptor/measurements/power_meter]
    {
      "num_values": 6,
      "units": ["V", "V", "V", "A", "A", "A"] /* voltage and current of phase 1, 2, 3 */
    }

[tedge/inventory/main/telemetry_descriptor/measurements/machinery_temperature]
    {
      "num_values": 1,
      "units": ["celsius"]
    }
```

### Use-Case 3: Announce **plugins** a device intends to use

**Description:**
The **main-device** and **child-devices** that intend to make use of **plugins**, can once announce their intend to the **inventory**. That way all **plugins** can be aware of all devices they need to serve. In addition those announcements contain all information the individual **plugin** needs to operate per **device** (e.g. a list of configuration-files for the "c8y_configuration_plugin").

**Examples  :**
```bash
# announce tedge_config for child-device "child1"
tedge mqtt pub -r \
  'tedge/inventory/child1/plugin_descriptor/tedge_config' \
  '
    {
      "files": [
         { "path": "/etc/tedge/tedge.toml", "type": "tedge.toml" },
         { "path": "/etc/tedge/mosquitto-conf/c8y-bridge.conf" },
         { "path": "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf" },
         { "path": "/etc/mosquitto/mosquitto.conf", type = "mosquitto", user = "mosquitto", group = "mosquitto", mode = 0o644 }
       ]
    }
  '
```
```bash
# announce tedge_config for child-device child2
tedge mqtt pub -r \
  'tedge/inventory/child2/plugin_descriptor/tedge_config' \
  '
    {
      "files": [
         { "path": "/etc/foo/foo.conf", "type": "foo.conf" },
         { "path": "/etc/foo/bar.conf", "type": "bar.conf" }
       ]
    }
  '
```
```bash
# announce tedge_log for child-device child2
tedge mqtt pub -r \
  'tedge/inventory/child2/plugin_descriptor/tedge_log' \
  '
    {
      /* TODO: tedge_log fields to be defined */
    }
  '
```

### Use-Case 4: Discover **devices** a **plugin** needs to serve

**Description:**
A **plugin** running on the **main-device** can discover the inventory for **devices** that intend to make use of the plugin.
That way the **plugin** can at the same time identify all **devices** it need to serve, as well as fetch all information it needs to operate per **device** (e.g. a list of configuration-files for the "c8y_configuration_plugin").

NOTE: Examples below assume announcements of _use-case 3_ above had happened before.

**Example:**
```bash
# the plugin "c8y_configuration_plugin" discovers all devices intend to make use of it,
# and receives the list of configuration files per device at the same time
tedge mqtt sub \
  'tedge/inventory/+/plugin_descriptor/tedge_config'
```
```bash
# Output:
[tedge/inventory/child1/plugin_descriptor/tedge_config]
    {
      "files": [
         { "path": "/etc/tedge/tedge.toml", "type": "tedge.toml" },
         { "path": "/etc/tedge/mosquitto-conf/c8y-bridge.conf" },
         { "path": "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf" },
         { "path": "/etc/mosquitto/mosquitto.conf", type = "mosquitto", user = "mosquitto", group = "mosquitto", mode = 0o644 }
       ]
    }

[tedge/inventory/child2/plugin_descriptor/tedge_config]
    {
      "files": [
         { "path": "/etc/foo/foo.conf", "type": "foo.conf" },
         { "path": "/etc/foo/bar.conf", "type": "bar.conf" }
       ]
    }
```

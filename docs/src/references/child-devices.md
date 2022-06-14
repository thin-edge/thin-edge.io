# Child Device Concept (Solution Proposal)

Thin-edge provides a child-device concept to 
TODO: Add motivation for child-device concept. Somethin like:
-- Device-side and Cloud-side structural view (that might match the physical structure)
-- Features are available per child-device (e.g. Software Management with one SW list per child device, ...)

* Child-Device support is provided on thin-edge public interfaces as below:
    - tedge/measurements
    - tedge/alarms
    - tedge/events
    - SW Management plugin API
    - Configuration Management
    - Logging Management
    
* Each public interface that provides child-device support has an optional field "child-device ID".

* Interfaces available on MQTT (as measurements, alarms, events) manage the field "child-device ID" as last topic of a message.
  Examples:
  
  ```
     tedge/measurements/<child-device ID>
     tedge/events/<event-type>/<child-device ID>
  ```

- Interface available via CLI (as SW Management Plugin API) manage the field "child-device ID" as optional argument in the command call.
  That means the command is executed locally on the thin-edge device, and has to take care to execute the remote action on the child-device.

  Examples:
  ```
    /etc/tedge/sm-plugins/<plugin-name>     install foo v1.2 /path/to/file <child-device ID>
    /etc/tedge/sm-plugins/CODESYS-handler   install foo v1.2 /path/to/file <child-device ID>
    /etc/tedge/sm-plugins/serial-programmer install foo v1.2 /path/to/file <child-device ID>
  ```
  Alternative with child-device ID in the path (avoids conflicts between filename over (child)devices), and allows thin-edge to gather a list of child devices (e.g. to ask each SW Man plugins for a Software List):
  ```
    /etc/tedge/sm-plugins/<child-device ID>/CODESYS-handler   install foo v1.2 /path/to/file
  ```

- If empty or no child-device ID is given, the thin-edge device is assumed to be meant.


TODO: Add restriction: "Those interfaces that are not provided via network (e.g. just via CLI) are not accessible from an external child device. That means for those a local process on the thin-edge device would be required."

TODO: Multilevel childs are not yet covered.

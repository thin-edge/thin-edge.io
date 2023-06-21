# A typical IIoT agent running thin-edge

TODO: a sketch highlighting:
- Complete equipment with hardware, software, sensors, network
- hardware: A main device + 2 child devices + 2 sensors per device + 2 actuators per device
- network: gateway + local network + OT bus
- mosquitto
- tedge-mapper
- tedge-device-management
- child-device agent running on a child device
- child-device agent running on the main device on behalf of a non-MQTT child device
- services running on the devices and the child devices

TODO: describe the agent introducing thin-edge specific concept
- child devices
- services
- mapper
- plugins
- out-of-the box versus provided by the agent developer

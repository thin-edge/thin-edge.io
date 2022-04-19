

Terminology 
- **platform**<br/>
  _Everything of thin-edge core_
- **application**<br/>
- **device**<br/>
- **device's system** or **OS**<br/>
- **reference device**<br/>
- **OT** and **OT-expert**<br/>
- **Device Owner**<br/>
- **API**<br/>
- **Interface**<br/>

TODO: Add definition to all terms above.
  
Explanation of requirenent's attributes:

- State: Shall be one of below: 
  - **draft**,              _newly created, and still under contruction_
  - **new**,                _new or modified, but not yet reviewed_
  - **ready for review**,   _requested to be reviewed_
  - **confirmed**,          _reviewed and confirmed_

All requirements below are grouped per six fundamental pillars:

**(a) Extensible & Adaptable** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| a.1 | The platform shall provide all features out of the box on the reference device with reference cloud.<br/><br/>**TODO: Do we want to have here a reference cloud (probably C8Y)? Could lead into too much push for that cloud (Hard to keep 'Cloud Agnostic')?** | Having minimal effort for device owners to make 1st steps with thin-edge. Scope: Demo and Prototype phase. |  | new |  |
| a.2 |  |   |  |   |  |
| a.3 | Thin-edge shall focus on functionality that can be highly re-used by most device owners.<br/><br/>**TODO: Define what 'highly reuse' does means here.** | To clearly focus with thin-edge design/implememtation on the _important_ things. Do not waste time with things that will not have any wide re-use, and instead might be heavily adapted/re-inveted in most customer cases. |  |  draft |  | 
| a.4 | A device owner shall be able to adapt and run the platform on any kind of Linux distribution. | The platform shall have no hard-coded dependencies to the device's linux distribution running on. I.E. no hard-coded dependencies to a package manager (e.g. apt), init manager (e.g. systemd), etc. Instead all needed dependencies shall be abstracted, e.g. using configuration files or executable plugins. |  | new |  |
| a.5 | **TODO: That's a placeholder to define how modular/configurable thin-edge core shall be.** | An advanced device owner shall be able to stick thin-edge functionality together and to adjust thin-edge in a way that it perfectly fits to it's device/use-case/application. I.e. to select exactly the needed functionality and disable others, and even allow to link thin-edge core to one static executable.<br/><br/>That is to disable/exclude unneeded components / code-paths to reduce complexity and minimize risc for malfunctions. |  | draft | |


**(b) Cloud Agnostic** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| b.1 |  |  |  |  |  |

**(c) Programming Language Agnostic** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| c.1 | The platform shall provide all public interfaces in a way that they can be used with all programming languages.<br/><br/>**TODO: Define what 'public' does mean here. And should we really say 'all'?** | Customers and Contributors shall not be forced to implement plugins or applications interfacing thin-edge in a specific programming language. Also thin-edge project shall not provide/maintain adapters (e.g Libraries) for various programming languages. |  | new |  |


**(d) Fit for (industrial) Embedded Systems** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| d.1 | The plaform shall consider resource constraints of embedded devices.<br/><br/>**TODO: Add magnitude of resources, and types of resources (e.g. RAM, disc).** | As thin-edge is made to connect thin-device to the cloud it shall run on PLCs or other Embedded Devices. |  | draft |  |
| d.2 | The platform shall use highly robust APIs for internal communication.<br/><br/>**TODO: Maybe add 'statically typed'?**) | Use of tight coupled communication channels (statically typed data-structures) instead of loose coupled communication (e.g. with freely defined MQTT message payload strings)  shall allow to detect missaligned communication partners early.<br/><br/> **TODO: maybe add "at compile-time and/or on component startup"**. |  | draft |  |
| d.3 | The platform shall provide public interfaces/APIs for seamless integration of OT applications and OT technologies.<br/><br/>**TODO: Or should that level of seamless integration not given by the core, but by 'bridges'?** | Thin-edge shall be the glue between OT and IT world. So OT experts (i.E. domain experts as e.g. PLC engineers, Embedded SW developers, ...) shall feel like home when using thin-edge's public interfaces. |  | draft |  |
| d.4 |  |  |  |  |  |


**(e) Debugging & Observability** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| e.1 |  |  |  |  |  |


**(f) Security** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| f.1 |  |  |  |  |  |


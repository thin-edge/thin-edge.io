

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
| a.1 | The platform shall provide all features out of the box on the reference device. | Having minimal effort for OT guys to make 1st steps with thin-edge. Scope: Demo and Prototype phase. |  | new |  |
| a.2 | The platform shall provide all features in a most re-usable way for a device-owner's specific device and application.| To avoid every Device Owners need not to reinvent the _same_ wheel to get core functionality running with it's Device & App. |  | new |  |
| a.3 | The platform must not implement things that will again significantly modified/reinvented by most device owners.<br/><br/>**TODO: Define what 'significantly' does means here.** | To clearly focus with thin-edge design/implememtation on the _important_ things. Do not waste time with things that will not have a wide re-use. |  |  new |  | 
| a.4 | A device owner shall be able to adapt and run the platform on any kind of Linux distribution. | The platform shall have no hard-coded dependencies to the device's linux distribution running on. I.E. no hard-coded dependencies to a package manager (e.g. apt), init manager (e.g. systemd), etc. Instead all needed dependencies shall be abstracted, e.g. using configuration files or executable plugins. |  | new |  |
| a.5 | **TODO: That's a placeholder to define how modular/configurable thin-edge core shall be.** | An advanced device owner shall be able to stick thin-edge functionality together and to adjust thin-edge in a way that it perfectly fits to it's device/use-case/application. |  | draft | |


**(b) Cloud Agnostic** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| b.1 |  |  |  |  |  |

**(c) Programming Language Agnostic** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| c.1 | The platform shall provide all public interaces in a way that they can be used with all programming languages.<br/><br/>**TODO: Define what 'public' does mean here.** | Customers and Contributors shall not be forced to implement plugins or applications interfacing thin-edge in a specific programming language. Also thin-edge project shall not provide/maintain adapters (e.g Libraries) for various programming languages. |  | new |  |


**(d) Fit for (industrial) Embedded Systems** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| d.1 | The plaform shall consider resource constraints of embedded devices.<br/><br/>**TODO: Add magnitude of resources, and types of resources.** |  |  | draft |  |
| d.2 | The platform shall use highly robust APIs for internal communication.<br/><br/>**TODO: Maybe add "built-time typed/verified"?**) | **TODO:** Instead of loose coupled communication, tight coupled communication channels shall detect missaligned communication partners early (?in best case at compile time?). |  | draft |  |
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


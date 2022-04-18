

Terminology 
- **platform**<br/>
  _Everything of thin-edge core_
- **application**<br/>
  TODO
- **device**<br/>
  TODO
- **device's system** or **OS**<br/>
  TODO
- **reference device**<br/>
  TODO
- **OT** and **OT-expert**<br/>
  TODO
- **Device Owner**<br/>
  TODO
- **API**<br/>
  TODO
- **Interface**<br/>
  TODO
  
  
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
| d.1 |  |  |  |  |  |


**(e) Debugging & Observability** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| e.1 |  |  |  |  |  |


**(f) Security** 
| ID | Name | Rationale | Stakeholder | Status | Comment |
|---|---|---|---|---|---|
| f.1 |  |  |  |  |  |


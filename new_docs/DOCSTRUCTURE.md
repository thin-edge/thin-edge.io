# doc-structure
this is a repository for the new documentation structure


-----------------------------------------------------------------------------


Chapter 1 Concepts of thin-edge.io 

(In this chapter terminology and concepts of thin-edge.io should be explained. Not only text should be used but also (simple/schematic) images. See the cumulocity concepts guide for examples. What we have now is too developer oriented, concepts should be more general described

    1.0 Overview - Generic concepts about thin-edge. In which scenarios thin-edge can be used. Thick Edge vs Thin edge, Graphical thin-edge architecture with all components.
    1.1 data model - Explain the data model, graphically and explain each data bucket
    1.2 runtime - What is the thin-edge runtime (components)
    1.3 cloud transport - Explain the concepts & challenges of sending data to IoT Platform clouds and how this can be address with the cloud transport
    1.4 mapper - Explain the concepts & challenges of mapping generic data to the thin-edge data model and cloud platforms and why this is necessary.
    1.5 plugins - Explain the thin-edge plugin concept how to extend it with additional features. 
    1.6 security - Explain how you can securely setup the thin-edge and communicate with cloud platforms using device certificates
    ...

    2.0 device management - Overview of device management and why it is very important for thin-edge use cases. (summary of the chapters following)
    2.1 software management - Explain the concept of software management and why it is important for some thin-edge use cases. Managing thin-edge software (packages, container, analytics rules etc.) using cloud platforms.
    2.2 configuration management - Explain the concept of configuration management and why it is important for some thin-edge use cases. Managing configurations of edges using cloud platforms
    2.3 log file management - Explain the concept of log management and why it is important for some thin-edge use cases. Managing & requesting log files from edges using cloud platforms.
    ...

Chapter 2 Getting started with thin-edge.io (a full walkthrough from first action to having an thin-edge.io running on a device

    2.1 Installation --> Concepts of thin-edge.io and how to install it on a reference platform
    2.2 Configuration --> Describe necessary configuration steps, refer to complex configuration in documents
    2.3 Run --> Run it on a reference device
    2.4 Develop & Build --> Describe how to contribute and make custom builds
    2.5 Extend --> Describe how thin-edge.io can be extend
    
 Chapter 3 How to guides
 (How-to guides explain details about specific features not providing a step-by-step approach)
 
    3.1 How to send MQTT messages
    3.2 
    etc.
 
 Chapter 4 Tutorials
 (Tutorials are a step-by-step approach with an end-to-end example)
    4.1 Use thin-edge.io on RevPi
  
 Chapter 5 Reference guide 
 (Reference guide is a kind of "Cheat sheet" which should contain all commands, topics etc. so you can search an use them in a efficient way )
 
 Chapter 6 API description
 (API documentation following standards if possilbe like Open API or other for MQTT topic structure and provided interfaces, SDK etc.)

 Chapter 7 Additional Information / Links
 How to get started with Cumulocity IoT, Azure IoT, AWS IoT and other clouds referring to their documentation and getting started guides.
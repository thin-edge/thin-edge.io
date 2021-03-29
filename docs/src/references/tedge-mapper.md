# The `tedge-mapper` daemon

This document lists the mqtt topics that are supported by the thin-edge.

## Thin Edge JSON Mqtt Topics
 To send the Thin Edge JSON measurements to a supported IoT cloud, the device should publish the measurements on 
 **/tedge/measurements** topic. Internally the tedge-mapper will consume the measurements from this topic, translates and sends
 them to the specific destination cloud.
 
## Cumulocity Mqtt Topics
The topics follow the below format  
`<protocol>/<direction><type>[/<template>][/<child id>] `   
 protocol                             
   s = standard               
   t = transient              
                                         
direction  
   u = upstream (from device)  
   d = downstream (to device)  
   e = error  
 
 type  
   s = static (built-in)  
   c = custom (device-defined)  
   d = default (defined in connect)  
   t = template  
   cr = credentials  

                                                   
   * Registration topics  
     c8y/s/dcr  - Subscribe to registration topic    
     c8y/s/ucr  - Publish registration topic  
 
   * Templates topics   
     c8y/s/dt   
     c8y/s/ut/#  

  * Static templates    
    c8y/s/us    
    c8y/t/us   
    c8y/q/us  
    c8y/c/us   
    c8y/s/ds  
    c8y/s/os  

  * Debug topics  
    c8y/s/e  

  * SmartRest2 topics  
    c8y/s/uc/#   
    c8y/t/uc/#  
    c8y/q/uc/#   
    c8y/c/uc/#   
    c8y/s/dc/#  
    c8ys/oc/#  

 * c8y JSON topics  
    c8y/measurement/measurements/create  
    c8y/error  

## Azure Mqtt Topics  
To send or to receive the messages from a Thin Edge device to Azure cloud over mqtt, clients should use the below topics.  

 * az/messages/events/  - Use this topic to send the messages from device to cloud.    
 * az/messages/devicebound/# - Use this topic to subscribe for the messages that were sent from cloud to device.     
 
 The Azure mqtt topics look as below  
 devices/{device_id}/messages/events/ - publish topic    
 devices/{device_id}/messages/devicebound/# - subscribe topic    
 Here the device_id is the Thin Edge device id, that was given while configuring the device.     
 To hide the complexity of the topics, alias were created.    

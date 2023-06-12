# Introduction
This document explains the scenarios that require the startup of different tedge daemons to be synchronized,
and solutions to avoid this synchronization by relying on dynamic runtime health status.

# Problem statement

Thin-edge.io has three main issues for synchronizing the daemons

1. The thin-edge daemons depend on another daemon's liveness to delegate or request some work.
 For example, tedge-mapper-c8y depends on tedge-agent. When the tedge-mapper-c8y is started,
 the mapper sends a request to get the existing software list on `tedge/commands/req/software/list` topic,
 if the tedge-agent is not up and running then the request message will be lost. So, it's better to check if the agent
 is up and running before sending the request to get the software list.

2. The thin-edge daemons are dependent on the liveness of the bridge to communicate with the cloud.
 For example, when the tedge-mapper-c8y comes up, it will try to get the jwt token. The request will be sent to c8y cloud
 over c8y/s/dat, if the bridge is not up then the request message will be lost.
 So, better to check the status of the bridge before sending the request to the c8y cloud.

3. Some of the daemons are dependent on the `tedge-mapper-c8y`(cloud mapper) file system to create the supported operations file.

In the existing solution, the thin-edge daemon interdependency was solved using a workaround by
creating the `persistent session` with the broker when the `daemon is installed`. 
For example, when the tedge-agent is installed, it will create a session with the MQTT broker. So, when the tedge-agent is down the broker
will buffer the messages and delivers once it comes up.

The problem with the existing workaround is
- The MQTT broker has to be up and running while installing the thin-edge components to create the session
- If in case the init session fails, then thin-edge daemons have to be started in a strict order,
 else the first message will be lost. i.e tedge-agent has to be started before the tedge-mapper-c8y
- Also, if the init session is done twice, then also the messages will be lost


# Proposed solutions and their Pros and Cons

## Proposal 1: Use the `health check` mechanism

The mapper uses the health-check mechanism to detect the health of another daemon and perform all the cloud-specific
 actions for that daemon like updating the supported operations etc.
In this, the `daemon` sends the health status `up` message once it's completely up. Then the `mapper` picks up this message and creates the required operation files and also sends the supported operation to the cloud.
Also, now the mapper can start interacting with the daemon.

Example 1,
- When tedge-mapper-c8y is started, it will subscribe to `tedge/health/#` topic
- When tedge-agent is started, it will publish health status as `up` on to `tedge/health/tedge-agent` topic
- Now the mapper picks up this message and creates the supported operations file (c8y_SoftwareUpdate, etc) in `/etc/tedge/operations/c8y` with their content. 
- Mapper sends the updated supported operations list to the c8y cloud.
- Also, now that the tedge-agent is up and running, the tedge-mapper-c8y will publish the request to get the
 software list on `tedge/commands/req/software/list`.
- Now the tedge-agent processes this request and sends the software list to the mapper, once the mapper gets the response it will forward it to the c8y cloud.

Example 2,
- When tedge-mapper-c8y is started, it will subscribe to `tedge/health/#` topic
- When c8y-log-plugin is started, it will publish health status as `up` on to `tedge/health/c8y-log-plugin` topic
- Now the mapper picks up this message and creates the c8y_LogfileRequest operation file in `/etc/tedge/operations/c8y`.
- Also the mapper creates the `c8y-log-plugin.toml` file, which contains the `supported log types` in `/etc/tedge/c8y`.
- Mapper sends the updated supported operations list to the c8y cloud. Also, it sends the supported `log types`(118,software-management) to the c8y cloud.

Pros: 
-   This will remove the file system dependency when the daemons want to create the supported operation files on the mapper file system.
    For example, tedge-agent wants to create the supported operations files in /etc/tedge/operations/c8y directory.

Cons: 
-   This leads to hard dependency on the list of daemons in the mapper. For example, the tedge-mapper-c8y has to subscribe to tedge/health/tedge-agent
topic to receive the health status from the tedge-agent. Based on this the mapper has to create a supported operation file in /etc/tedge/operations/c8y.
-   This will be a hard-coded dependency of the name of the plugin and can’t be updated on the fly if someone wants to change the name of the daemon
    or wants to add a new daemon.

## Proposal 2:  Introduce an `init message` mechanism

Make all the tedge components publish a `retained init` message describing their role (software management, config management, etc)
 and other details specific to that role.

The proposed topic will be `tedge/<device-id>/init/<cloud>`.
Here `<cloud>` could be c8y, azure, aws, etc

The message template is {Operation_name: <content>, Operation_name:<content>, Config_types: type1..typen}.
Here `Operation_name` is the name of the operation to be published to the cloud and 
the `content` is the content of the operation file, if any.
Here `Config_types` is the configuration types that are supported by that particular daemon.

Example 1:
- When the tedge-mapper-c8y is started, it will subscribe to `tedge/<device-id>/init/#` topic
- When tedge-agent is started, it will publish `{}` an empty `init` message on to `tedge/init/c8y/tedge-agent`
- Now the mapper picks up this message and creates the `c8y_Restart, c8y_SoftwareUpdat` supported operations file in `/etc/tedge/operations/c8y`. 
- Mapper sends the updated supported operations list to the c8y cloud.
- Also, now that the tedge-agent is up and running, the tedge-mapper-c8y will publish the request to get the
 software list on `tedge/commands/req/software/list`.
- Now the tedge-agent processes this request and sends the software list to the mapper, once the mapper gets the response it will forward it to the c8y cloud.

Example 2,
- When tedge-mapper-c8y is started, it will subscribe to `tedge/<device-id>/init/#` topic
- When c8y-log-plugin is started, it will publish {"LogfileRequest":"", "LogTypes": "software-management, mosquitto-log"} on to `tedge/thin-edge/init` topic
- Now the mapper picks up this message and creates the c8y_LogfileRequest operation file in `/etc/tedge/operations/c8y`.
- Also, the mapper creates the `c8y-log-plugin.toml` file, which contains the `supported log types` in `/etc/tedge/c8y`.
- Mapper sends the updated supported operations list to the c8y cloud. Also, it sends the supported `log types`(118,software-management, mosquitto-log) to the c8y cloud.

> Note: The mapper can also subscribe to cloud-specific init topics like `tedge/<device-id>/init/c8y`, to get a init message from a cloud-specific plugin,
that does not belong to any of the generic operations categories. For example, any custom operation plugin in case of c8y cloud.

Pros:
-   This will remove the file system dependency when the daemons want to create the supported operation files on the tedge-mapper-c8y file system.
-	No dependency on mapper, no hard-coded list of plugins
-	Very much cloud agnostic because the plugins are categorized based on roles not cloud-specific.
 For example, the `log` plugin can be used by c8y, azure, aws, etc.
 Based on the mapper type, the operations will be created in those specific directories and the supported operations list will be forwarded to the particular cloud.
    
Cons:
-   Need one more topic, and one more message format, which might be challenging to maintain.


# Shutting down or removing the plugin permanently
 
 When a plugin is shut down gracefully and removed from active operation, the operation must be removed from the list of operations supported by that 
 specific cloud.
 So, there might be a need for such a specific topic on which the plugin publishes its graceful exit and asks to remove the operation.

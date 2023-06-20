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

# Proposed solutions and their Pros and Cons

## Using the `health check` mechanism to synchronize `tedge-mapper-c8y` and `tedge-agent`

- The `tedge-mapper-c8y` can use the health-check mechanism to detect the health of the `tedge-agent` and then send the request to the `tedge-agent`.
- When the mapper starts it will subscribe to the `tedge/health/tedge-agent` topic.
- When the `tedge-agent` starts, it will publish a `health` status (up/down) message on the `tedge/health/tedge-agent` topic.
- Based on this `tedge-agent` health status message, the mapper can create the supported operations in the `/etc/tedge/operations/c8y` directory.
- Check if the `c8y-bridge` is up before sending the init messages, including `supported operations` list.
- Mapper will send the supported operations list to the c8y cloud.
- Mapper will send the request `software list` to `tedge-agent`.
- Once it receives a software list from `tedge-agent`, the mapper will send the list to the c8y cloud.

### Retrieving the internal-id
When the tedge-mapper-c8y starts, it will request the `internal id` of the `thin-edge` device from cumulocity cloud.
Which will be used by the mapper for further communication with the c8y cloud over HTTP.

To get the `internal id` over HTTP, the mapper needs the `JWT` token, which was retrieved over the `MQTT`.
So, before sending the request to retrieve the `JWT` token on the `c8y/s/uat` topic,
the mapper must check the health of the `c8y bridge`.
When the bridge is up, it can send a request for the JWT token.
Then the mapper will receive the `JWT` token on the `c8y/s/dat` topic.

If there is any issue in retrieving the JWT token then the mapper must `re-try` again.

Once the mapper possesses the JWT token, it can get the thin-edge device's internal-id.
If there is any issue with getting the internal-id then the mapper must `re-try` getting the
internal id.

### Sending the software update output to the c8y cloud

The software update/list operation result will be sent to the c8y cloud over `HTTPs`. 
If there is any issue sending the operation status and list, it must check the reason for the error and,
If the failure is due to the `internal-id` then it must get the internal id again and re-try sending the operation result.



## (Rejected Proposal) Proposal 2:  Introduce an `init message` mechanism

    ```admonish note
    Pre-condition for this proposal is that the `plugins` must be `cloud-agnostic` and the mappers will be `cloud-specific`.
    The plugins will be grouped based on the roles they perform. For example, software-management, device-restart, config-management, log-management, firmware-management,.etc.
    ```

    The tedge plugins must publish a `retained init` message describing their role (software-management, config management, etc)
     and other details specific to that role.

    The proposed topic will be `tedge/<device-id>/init/<cloud>`.
    Here `<cloud>` component is `optional` and could be c8y, azure, aws, etc

    The message template is {Operation_name: <content>, Operation_name:<content>, Configs: type1..typen}.
    Here `Operation_name` is the name of the operation to be published to the cloud and 
    the `content` is the content of the operation file, if any.
    Here `Configs` is the other configuration information that is to be passed to the cloud.
    For Example, in case of `log-plugin` the `Configs` is log types that are supporeted.

    Example 1:
    - When the tedge-mapper-c8y is started, it will subscribe to `tedge/<device-id>/init/#` topic
    - When tedge-agent is started, it will publish supported operations (software-management, device-restart) as part of `init` message on to `tedge/<device-id>/init` topic.
    - Now the c8y-mapper picks up this message and creates the `c8y_Restart, c8y_SoftwareUpdate` supported operations file in `/etc/tedge/operations/c8y`. 
    - Mapper sends the updated supported operations list (c8y_Restart, c8y_SoftwareUpdate) to the c8y cloud.
    - Also, now that the tedge-agent is up and running, the tedge-mapper-c8y will publish the request to get the
     software list on `tedge/commands/req/software/list`.
    - Now the tedge-agent processes this request and sends the software list to the mapper, once the mapper gets the response it will forward it to the c8y cloud.
    - Now onwards, when the operation request comes for the tedge-agent, if the operation present in the mapper's operation list, then it can forward the operation.

    Example 2,
    - When tedge-mapper-c8y is started, it will subscribe to `tedge/<device-id>/init` topic
    - When c8y-log-plugin is started, it will publish {"LogfileRequest":"", "Configs": "LogTypes": {"software-management, mosquitto-log"}} on to `tedge/thin-edge/init` topic
    - Now the mapper picks up this message and creates the c8y_LogfileRequest operation file in `/etc/tedge/operations/c8y`.
    - Mapper sends the updated supported operations list to the c8y cloud. Also, it sends the supported `log types`(118,software-management, mosquitto-log) to the c8y cloud.
    - When there is change in the `log types` the plugin must inform the new list to the mapper through the `init` topic,
     so that the supported logs types are reflected in the cloud side.

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

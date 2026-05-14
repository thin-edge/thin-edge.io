*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Suite Teardown       Get Logs

Test Tags           theme:c8y    theme:operation    theme:tedge-agent


*** Variables ***
${DEVICE_SN}    ${None}


*** Test Cases ***
Full supported operations message has no duplicates
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_pattern=114,c8y_DeviceProfile,c8y_DownloadConfigFile,c8y_LogfileRequest,c8y_RemoteAccessConnect,c8y_Restart,c8y_SoftwareUpdate,c8y_UploadConfigFile
    ...    minimum=1
    ...    maximum=1

Create and publish the tedge agent supported operations on mapper restart
    # stop mapper and remove the supported operations
    ThinEdgeIO.Stop Service    tedge-mapper-c8y
    Execute Command    sudo rm -rf /etc/tedge/operations/c8y/*

    # the operation files must not exist
    ThinEdgeIO.File Should Not Exist    /etc/tedge/operations/c8y/c8y_SoftwareUpdate
    ThinEdgeIO.File Should Not Exist    /etc/tedge/operations/c8y/c8y_Restart

    ${timestamp}=    Get Unix Timestamp
    # now restart the mapper
    ThinEdgeIO.start Service    tedge-mapper-c8y
    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-c8y/status/health
    ...    message_contains=up
    ...    date_from=${timestamp}
    # After receiving the health status `up` from tedge-agent, the mapper creates supported operations and will publish to c8y
    Should Have MQTT Messages    te/device/main/service/tedge-agent/status/health    message_contains=up

    # Check if the `c8y_SoftwareUpdate` and `c8y_Restart` ops files exists in `/etc/tedge/operations/c8y` directory
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/c8y/c8y_SoftwareUpdate
    ThinEdgeIO.File Should Exist    /etc/tedge/operations/c8y/c8y_Restart

    # Check if the tedge-agent supported operations exists in c8y cloud
    Cumulocity.Should Contain Supported Operations    c8y_Restart    c8y_SoftwareUpdate

Agent gets the software list request once it comes up
    ${timestamp}=    Get Unix Timestamp
    ThinEdgeIO.restart Service    tedge-agent
    # wait till there is up status on tedge-agent health
    Should Have MQTT Messages
    ...    te/device/main/service/tedge-agent/status/health
    ...    message_contains=up
    ...    date_from=${timestamp}
    # now there should be a new list request
    Should Have MQTT Messages
    ...    te/device/main///cmd/software_list/+
    ...    message_contains=status
    ...    date_from=${timestamp}

Re-publish supported operations by signal channel
    Execute Command    tedge mqtt pub -r te/device/child01// '{"@type": "child-device"}'
    Execute Command    tedge mqtt pub -r te/device/child01///cmd/restart '{}'
    Execute Command    tedge mqtt pub -r te/device/child01/service/foo '{"@type": "service"}'
    Execute Command    tedge mqtt pub -r te/device/child01/service/foo/cmd/restart '{}'

    Execute Command    tedge mqtt pub te/device/main/service/tedge-mapper-c8y/signal/sync '{}'
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_contains=114,c8y_DeviceProfile,c8y_DownloadConfigFile,c8y_LogfileRequest,c8y_RemoteAccessConnect,c8y_Restart,c8y_SoftwareUpdate,c8y_UploadConfigFile
    Should Have MQTT Messages    c8y/s/us/${DEVICE_SN}:device:child01    message_contains=114,c8y_Restart
    Should Have MQTT Messages    c8y/s/us/${DEVICE_SN}:device:child01:service:foo    message_contains=114,c8y_Restart

Recreate child supported operations after cloud deletion on mapper restart
    # register a child device and ensure it published supported operations
    Execute Command    tedge mqtt pub -r te/device/child02// '{"@type": "child-device"}'
    Execute Command    tedge mqtt pub -r te/device/child02///cmd/restart '{}'
    # Cumulocity.Should Have Exact Supported Operations    c8y/s/us/${DEVICE_SN}:device:child02    message_contains=114,c8y_Restart
    Cumulocity.Set Managed Object    ${DEVICE_SN}:device:child02
    Cumulocity.Should Have Exact Supported Operations    c8y_Restart

    # simulate operations being removed in Cumulocity - mapper should still maintain supported operation list for child device
    Execute Command    tedge mqtt pub c8y/s/us/${DEVICE_SN}:device:child02 114,
    Execute Command    sleep 1

    ${start}=    Get Unix Timestamp
    # restart the mapper so it re-publishes supported operations
    ThinEdgeIO.Restart Service    tedge-mapper-c8y

    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-c8y/status/health
    ...    message_contains=up
    ...    date_from=${start}
    Should Have MQTT Messages    te/device/main/service/tedge-agent/status/health    message_contains=up

    # confirm the child supported operations list is published again
    Should Have MQTT Messages    c8y/s/us/${DEVICE_SN}:device:child02    message_contains=114,c8y_Restart    date_from=${start}
    Cumulocity.Should Have Exact Supported Operations    c8y_Restart

Recreate nested child supported operations after cloud deletion on mapper restart
    Cumulocity.Device Should Have A Child Devices

    # register a child device childA and its child devices
    Execute Command    tedge mqtt pub -r te/device/childA// '{"@type": "child-device"}'
    Execute Command    tedge mqtt pub -r te/device/child0// '{"@type": "child-device", "@parent": "device/childA//"}'
    Execute Command    tedge mqtt pub -r te/device/child1// '{"@type": "child-device", "@parent": "device/childA//"}'
    Execute Command    tedge mqtt pub -r te/device/child2// '{"@type": "child-device", "@parent": "device/childA//"}'
    Execute Command    tedge mqtt pub -r te/device/child3// '{"@type": "child-device", "@parent": "device/childA//"}'
    Execute Command    tedge mqtt pub -r te/device/child4// '{"@type": "child-device", "@parent": "device/childA//"}'

    # set operations for all child devices
    Execute Command    tedge mqtt pub -r te/device/childA///cmd/restart '{}'
    Execute Command    tedge mqtt pub -r te/device/child0///cmd/restart '{}'
    Execute Command    tedge mqtt pub -r te/device/child1///cmd/restart '{}'
    Execute Command    tedge mqtt pub -r te/device/child2///cmd/restart '{}'
    Execute Command    tedge mqtt pub -r te/device/child3///cmd/restart '{}'
    Execute Command    tedge mqtt pub -r te/device/child4///cmd/restart '{}'

    # childA should be only direct child of main device
    Cumulocity.Device Should Have A Child Devices    ${DEVICE_SN}:device:childA

    # disable service and remove the devices on C8y, so registration messages need to be sent again
    Stop Service    tedge-mapper-c8y

    # wait until all in-flight MQTT messages to c8y are delivered and processed before we start deleting devices,
    # otherwise they will get recreated by incoming messages
    Execute Command    sleep 4

    ${mo}=    Cumulocity.Set Managed Object    ${DEVICE_SN}:device:childA
    Cumulocity.Set Managed Object    ${DEVICE_SN}
    Log    ${mo}

    Delete Managed Object    ${DEVICE_SN}:device:childA

    Cumulocity.Device Should Not Have Any Child Devices

    # we restart the mapper, devices should be re-registered and have the same hierarchy
    Start Service    tedge-mapper-c8y

    Cumulocity.Device Should Have A Child Devices    ${DEVICE_SN}:device:childA

    Cumulocity.Set Managed Object    ${DEVICE_SN}:device:childA
    # tried to test with more child devices but this keyword can return max 5 items because default paging in
    # assertions library
    Cumulocity.Device Should Have A Child Devices    ${DEVICE_SN}:device:child0
    ...    ${DEVICE_SN}:device:child1
    ...    ${DEVICE_SN}:device:child2
    ...    ${DEVICE_SN}:device:child3
    ...    ${DEVICE_SN}:device:child4

    [Teardown]    Cumulocity.Set Managed Object    ${DEVICE_SN}


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup
    VAR    ${DEVICE_SN}    ${device_sn}    scope=SUITE
    Device Should Exist    ${DEVICE_SN}

Delete Managed Object
    [Documentation]    Cumulocity.Delete Managed Object takes MO ID but we only know its external id, which is different.
    ...    This keyword takes external id, gets MO ID, and deletes the MO.
    [Arguments]    ${external_id}
    ${mo}=    Cumulocity.Set Managed Object    ${external_id}
    VAR   ${mo_id}    ${mo}[id]
    Log    ${mo_id}
    Cumulocity.Delete Managed Object    ${mo_id}
    [Teardown]    Cumulocity.Set Managed Object    ${DEVICE_SN}

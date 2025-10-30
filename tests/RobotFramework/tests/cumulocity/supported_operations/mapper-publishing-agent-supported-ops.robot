*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs
Test Timeout        5 minutes

Test Tags           theme:c8y    theme:operation    theme:tedge-agent


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


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

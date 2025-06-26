*** Settings ***
Resource            ../../resources/common.resource
Library             DateTime
Library             Cumulocity
Library             ThinEdgeIO

Test Teardown       Get Logs


*** Test Cases ***
No unexpected child devices created with service autostart
    [Tags]    \#2584
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    systemctl start mosquitto
    Execute Command    systemctl start tedge-agent
    Execute Command    systemctl start tedge-mapper-c8y
    Execute Command    tedge connect c8y
    Device Should Exist    ${DEVICE_SN}

    # wait for messages to be processed
    Sleep    15s

    # Assert that there are no child devices present.
    Cumulocity.Device Should Not Have Any Child Devices

No unexpected child devices created without service autostart
    [Tags]    \#2606
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}

    # Touching the operations directories should not create child devices
    Execute Command    touch /etc/tedge/operations
    Execute Command    touch /etc/tedge/operations/c8y

    # wait for fs event to be detected
    Sleep    5s

    # Assert that there are no child devices present.
    Cumulocity.Device Should Not Have Any Child Devices

Mapper restart does not alter device hierarchy
    [Tags]    \#2409

    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}

    ${child_level1}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${child_level1}//' '{"@id":"${child_level1}","@type":"child-device","@parent":"device/main//","name":"${child_level1}"}'
    ${child_level2}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${child_level2}//' '{"@id":"${child_level2}","@type":"child-device","@parent":"device/${child_level1}//","name":"${child_level2}"}'

    Set Device    ${DEVICE_SN}
    Device Should Have A Child Devices    ${child_level1}
    Set Device    ${child_level1}
    Device Should Have A Child Devices    ${child_level2}

    Restart Service    tedge-mapper-c8y

    Set Device    ${DEVICE_SN}
    Device Should Have A Child Devices    ${child_level1}
    Set Device    ${child_level1}
    Device Should Have A Child Devices    ${child_level2}

Mapper started early does not miss supported operations
    [Tags]    \#2689
    ${DEVICE_SN}=    Setup    connect=${False}
    Execute Command    systemctl start mosquitto
    Execute Command    systemctl start tedge-agent
    Execute Command    systemctl start tedge-mapper-c8y
    Execute Command    tedge connect c8y
    Device Should Exist    ${DEVICE_SN}

    # Assert that there are no child devices present.
    Cumulocity.Should Contain Supported Operations
    ...    c8y_Restart
    ...    c8y_SoftwareUpdate
    ...    c8y_UploadConfigFile
    ...    c8y_DownloadConfigFile
    ...    c8y_LogfileRequest

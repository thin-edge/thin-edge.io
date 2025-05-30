*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:telemetry


*** Test Cases ***
Update Inventory data via inventory.json
    ${mo}=    Cumulocity.Device Should Have Fragments    customData    types
    Should Be Equal    ${mo["customData"]["mode"]}    ACTIVE
    Should Be Equal As Integers    ${mo["customData"]["version"]}    ${1}
    Should Be Equal    ${mo["customData"]["alertingEnabled"]}    ${True}
    Should Be Equal    ${mo["types"][0]}    type1
    Should Be Equal    ${mo["types"][1]}    type2
    Should Be Equal    ${mo["name"]}    TST_dark_knight
    Should Be Equal    ${mo["type"]}    RPi5

Inventory includes the agent fragment with version information
    ${expected_version}=    Execute Command    tedge-mapper --version | cut -d' ' -f2    strip=${True}
    ${mo}=    Cumulocity.Device Should Have Fragments    c8y_Agent
    Should Be Equal    ${mo["c8y_Agent"]["name"]}    thin-edge.io
    Should Be Equal    ${mo["c8y_Agent"]["version"]}    ${expected_version}
    Should Be Equal    ${mo["c8y_Agent"]["url"]}    https://thin-edge.io

Update main device inventory fragments via twin topics
    Validate inventory fragment updates via twin topics    ${DEVICE_SN}    main

Update child device inventory fragments via twin topics
    ${child_sn}=    Get Random Name
    Execute Command    tedge mqtt pub --retain 'te/device/${child_sn}//' '{"@type":"child-device","@id":"${child_sn}"}'

    Validate inventory fragment updates via twin topics    ${child_sn}

Update nested child device inventory fragments via twin topics
    ${direct_child}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${direct_child}//' '{"@type":"child-device","@id":"${direct_child}"}'

    ${nested_child}=    Get Random Name
    Execute Command
    ...    tedge mqtt pub --retain 'te/device/${nested_child}//' '{"@type":"child-device","@parent":"device/${direct_child}//","@id":"${nested_child}"}'

    Validate inventory fragment updates via twin topics    ${nested_child}

Update main device inventory fragments via twin topics using built-in bridge
    ThinEdgeIO.Execute Command    tedge config set mqtt.bridge.built_in true
    ThinEdgeIO.Execute Command    tedge config set c8y.bridge.topic_prefix custom-c8y-prefix
    ThinEdgeIO.Execute Command    tedge reconnect c8y
    Service Health Status Should Be Up    tedge-mapper-custom-c8y-prefix
    ${output}=    Execute Command    sudo tedge connect c8y --test    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Connection check to c8y cloud is successful.

    Validate inventory fragment updates via twin topics    ${DEVICE_SN}    main


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/inventory.json    /etc/tedge/device/
    ThinEdgeIO.Disconnect Then Connect Mapper    c8y

Validate inventory fragment updates via twin topics
    [Arguments]    ${device_xid}=${DEVICE_SN}    ${device_tid}=${device_xid}

    Execute Command    tedge mqtt pub --retain "te/device/${device_tid}///twin/subtype" '"LinuxDeviceA"'
    Execute Command    tedge mqtt pub --retain "te/device/${device_tid}///twin/type" '"NewType"'
    Execute Command    tedge mqtt pub --retain "te/device/${device_tid}///twin/name" '"NewName"'
    Execute Command
    ...    tedge mqtt pub --retain "te/device/${device_tid}///twin/device_OS" '{"family":"Debian","version":11,"complex":[1,"2",3],"object":{"foo":"bar"}}'

    Cumulocity.Set Device    ${device_xid}
    Device Should Have Fragment Values    subtype\="LinuxDeviceA"
    Device Should Have Fragment Values    type\="NewType"
    Device Should Have Fragment Values    name\="NewName"

    ${mo}=    Device Should Have Fragments    device_OS
    Should Be Equal    ${mo["device_OS"]["family"]}    Debian
    Should Be Equal As Integers    ${mo["device_OS"]["version"]}    11

    Should Be Equal As Integers    ${mo["device_OS"]["complex"][0]}    1
    Should Be Equal As Strings    ${mo["device_OS"]["complex"][1]}    2
    Should Be Equal As Integers    ${mo["device_OS"]["complex"][2]}    3
    Should Be Equal    ${mo["device_OS"]["object"]["foo"]}    bar

    # Validate clearing of fragments
    Execute Command    tedge mqtt pub --retain "te/device/${device_tid}///twin/device_OS" ''
    Managed Object Should Not Have Fragments    device_OS
    Execute Command    tedge mqtt pub --retain "te/device/${device_tid}///twin/subtype" ''
    Managed Object Should Not Have Fragments    subtype

    # Validate `name` and `type` can't be cleared
    Execute Command    tedge mqtt pub --retain "te/device/${device_tid}///twin/type" ''
    Execute Command    tedge mqtt pub --retain "te/device/${device_tid}///twin/name" ''
    Sleep
    ...    5s
    ...    reason=Wait a minimum period before checking that the fragment has not changed (as it was previously set)
    Device Should Have Fragment Values    type\="NewType"
    Device Should Have Fragment Values    name\="NewName"

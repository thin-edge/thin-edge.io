*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:software    theme:tedge_mapper


*** Test Cases ***
Restarting the mapper while a software list request is pending does not create new requests
    # With the agent stopped, the request issued by the mapper is never processed nor cleared
    ThinEdgeIO.Stop Service    tedge-agent
    Repeat Keyword    10 times    Restart Mapper And Wait For Software Update Registration
    Retained Software List Requests Should Be    1


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}
    # The software list requested on the first startup is processed and cleared
    Wait Until Keyword Succeeds    30s    1s    Retained Software List Requests Should Be    0

Restart Mapper And Wait For Software Update Registration
    ${timestamp}=    Get Unix Timestamp
    ThinEdgeIO.Restart Service    tedge-mapper-c8y
    # The supported operations are published along with the software list request, if any
    Should Have MQTT Messages
    ...    c8y/s/us
    ...    message_pattern=^114,.*c8y_SoftwareUpdate.*
    ...    date_from=${timestamp}

Retained Software List Requests Should Be
    [Arguments]    ${expected_count}
    ${output}=    Execute Command
    ...    mosquitto_sub --retained-only -W 3 -t "te/device/main///cmd/software_list/+" -v
    ...    exp_exit_code=27
    ...    return_stdout=True
    Length Should Be    ${output.splitlines()}    ${expected_count}

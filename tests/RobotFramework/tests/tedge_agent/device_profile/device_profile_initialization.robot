*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent


*** Test Cases ***
Device profile is included in supported operations
    Should Have MQTT Messages    te/device/main///cmd/device_profile    message_pattern=^{}$    date_from=-5s
    Should Contain Supported Operations    c8y_DeviceProfile

Workflow definition is created along with template
    ${original_config}    Execute Command    cat /etc/tedge/operations/device_profile.toml
    ${original_template}    Execute Command    cat /etc/tedge/operations/device_profile.toml.template

    Should Be Equal    ${original_config}    ${original_template}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}    Setup
    Set Suite Variable    ${DEVICE_SN}
    Device Should Exist    ${DEVICE_SN}

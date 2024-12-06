*** Settings ***
Resource            ../../../../resources/common.resource
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:configuration


*** Test Cases ***
Support Legacy Configuration Download Operation
    [Documentation]    Support Cumulocity Legacy File Upload/Download
    ...    which was the default before file-type configuration operations were introduced
    ...    Cumulocity Docs: https://cumulocity.com/docs/device-integration/fragment-library/#install-legacy-configuration

    # pre-condition
    Cumulocity.Managed Object Should Not Have Fragments    c8y_ConfigurationDump

    # Send configuration to a device
    ${binary_url}=    Create Inventory Binary    dummy    example    contents=foobar
    ${binary_id}=    Get Binary ID From URL    ${binary_url}
    ${operation}=    Cumulocity.Create Operation
    ...    description=Legacy Configuration Download
    ...    fragments={"c8y_DownloadConfigFile":{"url": "${binary_url}", "c8y_ConfigurationDump": {"id": "${binary_id}"}}}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}

    # Server will update the inventory managed object fields
    Cumulocity.Device Should Have Fragments    c8y_ConfigurationDump
    Cumulocity.Device Should Have Fragment Values    c8y_ConfigurationDump.id\="${binary_id}"

    # Get configuration from the device
    ${operation}=    Cumulocity.Create Operation
    ...    description=Legacy Configuration Upload
    ...    fragments={"c8y_UploadConfigFile":{}}
    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=False
    Set Suite Variable    $DEVICE_SN

    Transfer To Device
    ...    ${CURDIR}/tedge-configuration-plugin.toml
    ...    /etc/tedge/plugins/tedge-configuration-plugin.toml
    Cumulocity.Device Should Exist    ${DEVICE_SN}

Get Binary ID From URL
    [Arguments]    ${url}
    ${parts}=    Split String From Right    ${url}    separator=/    max_split=1
    RETURN    ${parts[1]}

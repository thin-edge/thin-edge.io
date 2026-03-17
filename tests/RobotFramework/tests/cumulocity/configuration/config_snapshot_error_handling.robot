*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:configuration


*** Variables ***
${DEVICE_SN}            ${EMPTY}
${FILE_TRANSFER_DIR}    /var/tedge/file-transfer


*** Test Cases ***
Config snapshot fails immediately when file-transfer storage is not writable
    [Documentation]    When the local file-transfer service cannot write the uploaded file (e.g. due
    ...    to a permissions problem on its storage directory) it returns HTTP 500.
    ...    The config snapshot operation should immediately transition to the FAILED state instead of
    ...    retrying indefinitely (which would leave it stuck in EXECUTING for up to 5 minutes).
    [Tags]    \#3374
    [Setup]    Remove Write Permissions From File-Transfer Directory

    Cumulocity.Set Device    ${DEVICE_SN}

    # The timeout here is intentionally short: if the fix is in place the operation
    # should fail within a few seconds; without the fix it would keep retrying for
    # ~5 minutes (the default max_elapsed_time in the upload backoff) before giving up.
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin
    Operation Should Be FAILED
    ...    ${operation}
    ...    failure_reason=config-manager failed uploading configuration snapshot.*
    ...    timeout=30
    [Teardown]    Run Keywords
    ...    Restore File-Transfer Directory Permissions
    ...    AND    Get Logs


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Cumulocity.Device Should Exist    ${DEVICE_SN}

Remove Write Permissions From File-Transfer Directory
    # Remove all permissions from the file-transfer storage directory so that any
    # PUT request to the file-transfer service results in a 500 Internal Server Error.
    Execute Command    sudo chmod 000 ${FILE_TRANSFER_DIR}

Restore File-Transfer Directory Permissions
    Execute Command    sudo chmod 755 ${FILE_TRANSFER_DIR}

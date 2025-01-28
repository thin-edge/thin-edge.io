*** Settings ***
Documentation       Run connection test while being connected and check the positive response in stdout
...                 disconnect the device from cloud and check the negative message in stderr
...                 Run sudo tedge connect c8y and check

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Suite Setup
Suite Teardown      Get Logs

Test Tags           theme:cli    theme:mqtt    theme:c8y


*** Test Cases ***
tedge_connect_test_positive
    Execute Command    sudo tedge connect c8y || true    # Connect but don't fail if already connected
    ${output}=    Execute Command    sudo tedge connect c8y --test
    Should Contain    ${output}    Connection check to c8y cloud is successful.

Non-root users should be able to read the mosquitto configuration files #2154
    [Tags]    \#2154
    Execute Command    sudo tedge connect c8y || true
    Should Have File Permissions    /etc/tedge/mosquitto-conf/tedge-mosquitto.conf    644 root:root
    Execute Command    sudo tedge config set mqtt.bridge.built_in false
    Execute Command    sudo tedge reconnect c8y
    Should Have File Permissions    /etc/tedge/mosquitto-conf/tedge-mosquitto.conf    644 root:root
    Should Have File Permissions    /etc/tedge/mosquitto-conf/c8y-bridge.conf    644 root:root
    # Reset things after running the test
    [Teardown]    Execute Command    sudo tedge config unset mqtt.bridge.built_in

tedge_connect_test_negative
    Execute Command    sudo tedge disconnect c8y
    ${output}=    Execute Command
    ...    sudo tedge connect c8y --test
    ...    exp_exit_code=1
    ...    stdout=${False}
    ...    stderr=${True}

tedge_connect_test_sm_services
    ${output}=    Execute Command    sudo tedge connect c8y
    Should Contain    ${output}    Enabling tedge-agent... ✓
    Should Contain    ${output}    Enabling tedge-mapper-c8y... ✓
    Should Not Contain
    ...    ${output}
    ...    Warning:
    ...    message=Warning should not be displayed if the port does not match. Issue #2863

tedge_disconnect_test_sm_services
    ${output}=    Execute Command    sudo tedge disconnect c8y
    Should Not Contain    ${output}    Disabling tedge-agent... ✓
    Should Contain    ${output}    Disabling tedge-mapper-c8y... ✓

tedge reconnect does not restart agent
    ${pid_before}=    Execute Command    sudo systemctl show --property MainPID tedge-agent
    ${output}=    Execute Command    sudo tedge reconnect c8y
    ${pid_after}=    Execute Command    sudo systemctl show --property MainPID tedge-agent
    Should Be Equal    ${pid_before}    ${pid_after}

tedge reconnect restarts mapper
    ${pid_before}=    Execute Command    sudo systemctl show --property MainPID tedge-mapper-c8y
    ${output}=    Execute Command    sudo tedge reconnect c8y
    ${pid_after}=    Execute Command    sudo systemctl show --property MainPID tedge-mapper-c8y
    Should Not Be Equal    ${pid_before}    ${pid_after}

Check absence of OpenSSL Error messages #3024
    Skip
    ...    msg=This test is flaky. There is client (yet to be identified) that fails to connect on port 8883 leading to OpenSSL Error
    ${SuiteStart}=    Get Suite Start Time
    # Only checkout output if mosquitto is being used
    ${output}=    Execute Command
    ...    systemctl is-active mosquitto && journalctl -u mosquitto -n 5000 --since "@${SuiteStartSeconds}" || true
    Should Not Contain    ${output}    OpenSSL Error


*** Keywords ***
Suite Setup
    Setup
    ${SuiteStartSeconds}=    Get Unix Timestamp
    Set Suite Variable    $SuiteStartSeconds

Should Have File Permissions
    [Arguments]    ${file}    ${expected_permissions}
    ${FILE_MODE_OWNERSHIP}=    Execute Command    stat -c '%a %U:%G' ${file}    strip=${True}
    Should Be Equal
    ...    ${FILE_MODE_OWNERSHIP}
    ...    ${expected_permissions}
    ...    msg=Unexpected file permissions/ownership of ${file}

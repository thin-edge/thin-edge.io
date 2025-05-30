*** Settings ***
Documentation       Run connection test while being connected and check the positive response in stdout
...                 disconnect the device from cloud and check the negative message in stderr
...                 Run sudo tedge connect c8y and check

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Suite Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:cli    theme:mqtt    theme:c8y


*** Test Cases ***
tedge_connect_test_positive
    Execute Command    sudo tedge connect c8y || true    # Connect but don't fail if already connected
    ${output}=    Execute Command    sudo tedge connect c8y --test    stdout=${False}    stderr=${True}
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
    ${output}=    Execute Command    sudo tedge connect c8y    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Enabling tedge-agent... ✓
    Should Contain    ${output}    Enabling tedge-mapper-c8y... ✓
    Should Not Contain
    ...    ${output}
    ...    Warning:
    ...    message=Warning should not be displayed if the port does not match. Issue #2863

tedge_disconnect_test_sm_services
    ${output}=    Execute Command    sudo tedge disconnect c8y    stdout=${False}    stderr=${True}
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

tedge reconnect validates and uses the new certificate if any
    Renew certificate
    ${output}=    Execute Command    sudo tedge reconnect c8y    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Validating new certificate
    Should Contain    ${output}    The new certificate is now the active certificate
    ${output}=    Execute Command    tedge cert show c8y --new    exp_exit_code=1    stderr=True
    Should Contain    ${output}[1]    No such file

tedge connect validates and uses the new certificate if any
    Renew certificate
    Execute Command    sudo tedge disconnect c8y
    ${output}=    Execute Command    sudo tedge connect c8y    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Validating new certificate
    Should Contain    ${output}    The new certificate is now the active certificate
    ${output}=    Execute Command    tedge cert show c8y --new    exp_exit_code=1    stderr=True
    Should Contain    ${output}[1]    No such file

tedge connect skips new certificate validation if connected
    Renew certificate
    # The connection is rejected because already connected
    ${output}=    Execute Command    sudo tedge connect c8y    exp_exit_code=1    stderr=True
    Should Contain    ${output}[1]    Connection is already established
    # The invalid certificate is left unchanged
    ${new-cert}=    Execute Command    tedge cert show c8y --new
    Should Contain    ${new-cert}    O=thin-edge, OU=Test Device

tedge reconnect rejects any new invalid certificate
    Renew certificate
    Execute Command    echo garbage | sudo tee "$(tedge config get c8y.device.cert_path).new"
    # The connection should be successful, despite the new certificate being invalid
    ${output}=    Execute Command    sudo tedge reconnect c8y    exp_exit_code=3    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Validating new certificate
    Should Contain    ${output}    Connection error while creating device
    Should Contain    ${output}    attempt 2 of 3
    Should Contain    ${output}    attempt 3 of 3
    ${output}=    Execute Command    sudo tedge connect c8y --test    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Connection check to c8y cloud is successful
    # The invalid certificate is left unchanged
    ${new-cert}=    Execute Command    cat "$(tedge config get c8y.device.cert_path).new"
    Should Contain    ${new-cert}    garbage


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

Renew certificate
    # Simulate certificate renewal
    Execute Command
    ...    sudo cp "$(tedge config get c8y.device.cert_path)" "$(tedge config get c8y.device.cert_path).new"
    ${output}=    Execute Command    tedge cert show c8y --new
    Should Contain    ${output}    O=thin-edge, OU=Test Device

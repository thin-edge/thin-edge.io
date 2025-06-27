*** Settings ***
Documentation       Run certificate upload test and fails to upload the cert because there is no root certificate directory,
...                 Then check the negative response in stderr

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Teardown       Get Logs

Test Tags           theme:cli    theme:mqtt    theme:c8y    adapter:docker


*** Test Cases ***
tedge cert upload c8y respects root cert path
    [Setup]    Setup With Self-Signed Certificate
    Execute Command    sudo tedge disconnect c8y
    ${output}=    Execute Command
    ...    sudo tedge cert renew --ca self-signed
    ...    stderr=${True}
    ...    stdout=${False}
    ...    ignore_exit_code=${True}
    Should Contain
    ...    ${output}
    ...    Certificate renewed successfully
    Should Contain
    ...    ${output}
    ...    the new certificate has to be uploaded to the cloud
    Execute Command    mv /etc/ssl/certs /etc/ssl/certs_test
    Execute Command    tedge config set c8y.root_cert_path /etc/ssl/certs_test
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y
    ${output}=    Execute Command    sudo tedge connect c8y    stdout=${False}    stderr=${True}
    Should Contain    ${output}    Verifying device is connected to cloud... ✓
    Should Contain    ${output}    Checking Cumulocity is connected to intended tenant... ✓


*** Keywords ***
Setup With Self-Signed Certificate
    ${DEVICE_SN}=    Setup    register_using=self-signed
    Set Test Variable    $DEVICE_SN

*** Settings ***
Documentation    Run certificate upload test and fails to upload the cert because there is no root certificate directory,
...              Then check the negative response in stderr              

Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:mqtt    theme:c8y    adapter:docker
Test Teardown         Get Logs

*** Test Cases ***
tedge cert upload c8y respects root cert path
    [Setup]    Setup With Self-Signed Certificate
    Execute Command    sudo tedge disconnect c8y
    ${output}=    Execute Command    sudo tedge cert renew    stderr=${True}    stdout=${False}    ignore_exit_code=${True}
    Should Contain    ${output}    Certificate was successfully renewed, for un-interrupted service, the certificate has to be uploaded to the cloud
    Execute Command    mv /etc/ssl/certs /etc/ssl/certs_test
    Execute Command    tedge config set c8y.root_cert_path /etc/ssl/certs_test
    Execute Command    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y
    ${output}=    Execute Command    sudo tedge connect c8y
    Should Contain    ${output}    Connection check is successful.

*** Keywords ***
Setup With Self-Signed Certificate
    ${DEVICE_SN}=                    Setup    skip_bootstrap=${True}
    Set Test Variable               $DEVICE_SN
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --cert-method selfsigned

*** Settings ***
Documentation       Run certificate upload test and fails to upload the cert because there is no root certificate directory,
...                 Then check the negative response in stderr

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Teardown       Get Logs

Test Tags           theme:cli    theme:mqtt    theme:c8y


*** Test Cases ***
Create the certificate
    [Setup]    Setup With Self-Signed Certificate
    # You can then check the content of that certificate.
    ${output}=    Execute Command    sudo tedge cert show    # You can then check the content of that certificate.
    Should Contain
    ...    ${output}
    ...    Certificate: /etc/tedge/device-certs/tedge-certificate.pem
    ...    collapse_spaces=true
    Should Contain
    ...    ${output}
    ...    Subject: CN=${DEVICE_SN}, O=Thin Edge, OU=Device
    ...    collapse_spaces=true
    Should Contain
    ...    ${output}
    ...    Issuer: CN=${DEVICE_SN}, O=Thin Edge, OU=Device
    ...    collapse_spaces=true
    Should Contain    ${output}    Status:
    Should Contain    ${output}    VALID
    Should Contain    ${output}    Valid from:
    Should Contain    ${output}    Valid until:
    Should Contain    ${output}    Serial number:
    Should Contain    ${output}    Thumbprint:

Renew the certificate
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
    Execute Command
    ...    sudo env C8YPASS\='${C8Y_CONFIG.password}' tedge cert upload c8y --user ${C8Y_CONFIG.username}
    ...    log_output=${False}
    ${output}=    Execute Command    sudo tedge connect c8y
    Should Contain    ${output}    Verifying device is connected to cloud... ✓
    Should Contain    ${output}    Checking Cumulocity is connected to intended tenant... ✓

Cert upload prompts for username (from stdin)
    # Note: Use bash process substitution to simulate user input from /dev/stdin
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
    Execute Command
    ...    cmd=sudo env --unset=C8Y_USER C8Y_PASSWORD='${C8Y_CONFIG.password}' bash -c "tedge cert upload c8y < <(echo '${C8Y_CONFIG.username}')"
    ...    log_output=${False}
    ${output}=    Execute Command    sudo tedge connect c8y
    Should Contain    ${output}    Verifying device is connected to cloud... ✓
    Should Contain    ${output}    Checking Cumulocity is connected to intended tenant... ✓

Cert upload supports reading username/password from go-c8y-cli env variables
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
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y
    ...    log_output=${False}
    ${output}=    Execute Command    sudo tedge connect c8y
    Should Contain    ${output}    Verifying device is connected to cloud... ✓
    Should Contain    ${output}    Checking Cumulocity is connected to intended tenant... ✓

Renew certificate fails
    [Setup]    Setup Without Certificate
    Execute Command    sudo tedge cert remove
    ${output}=    Execute Command
    ...    sudo tedge cert renew --ca self-signed
    ...    stderr=${True}
    ...    stdout=${False}
    ...    ignore_exit_code=${True}
    Should Contain    ${output}    Missing file: "/etc/tedge/device-certs/tedge-certificate.pem"
    # Restore the certificate
    Execute Command    sudo tedge cert create --device-id test-user

tedge cert upload c8y command fails
    [Setup]    Setup Without Certificate
    Execute Command    sudo tedge cert create --device-id test-user
    Execute Command    sudo tedge config set c8y.url example.c8y.com
    Execute Command    tedge config set c8y.root_cert_path /etc/ssl/certs_test
    ${output}=    Execute Command
    ...    sudo env C8YPASS\='password' tedge cert upload c8y --user testuser
    ...    ignore_exit_code=${True}
    ...    stdout=${False}
    ...    stderr=${True}
    Execute Command    tedge config unset c8y.root_cert_path
    Should Contain
    ...    ${output}
    ...    Unable to read certificates from c8y.root_cert_path: failed to read from path "/etc/ssl/certs_test"


*** Keywords ***
Setup With Self-Signed Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --cert-method selfsigned

Setup Without Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --install --no-bootstrap --no-connect

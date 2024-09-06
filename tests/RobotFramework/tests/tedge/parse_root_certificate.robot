*** Settings ***
Resource            ../../resources/common.resource
Library             Collections
Library             String
Library             ThinEdgeIO

Test Teardown       Get Logs

Test Tags           theme:ca_certificates


*** Test Cases ***
Verify Single Certificate File
    [Setup]    Setup With Self-Signed Certificate
    ThinEdgeIO.File Should Exist    /etc/tedge/device-certs/tedge-certificate.pem
    ${output_cert}=    Execute Command    cat /etc/tedge/device-certs/tedge-certificate.pem
    Should Contain    ${output_cert}    -----BEGIN CERTIFICATE-----

Verify Single Private Key File
    [Setup]    Setup With Self-Signed Certificate
    ThinEdgeIO.File Should Exist    /etc/tedge/device-certs/tedge-private-key.pem
    ${output_key}=    Execute Command    cat /etc/tedge/device-certs/tedge-private-key.pem
    Should Contain    ${output_key}    -----BEGIN PRIVATE KEY-----

Verify Multiple Certificates in Directory
    [Setup]    Setup With Self-Signed Certificate
    ThinEdgeIO.File Should Exist    /etc/tedge/device-certs/tedge-certificate.pem
    ${dir_contents}=    Execute Command    ls /etc/tedge/device-certs
    Log    ${dir_contents}

    # List .pem files and check the result
    ${cert_files}=    Execute Command    ls /etc/tedge/device-certs/*.pem
    ${cert_files_list}=    Split String    ${cert_files}    \n
    @{filtered_cert_files}=    Create List
    FOR    ${file}    IN    @{cert_files_list}
        IF    '${file}' != ''
            Append To List    ${filtered_cert_files}    ${file}
        END
    END

    ${cert_files_length}=    Get Length    ${filtered_cert_files}
    Should Be True    ${cert_files_length} > 1
    FOR    ${cert_file}    IN    @{filtered_cert_files}
        ${output_cert}=    Execute Command    cat ${cert_file}
        Should Contain Any Line    ${output_cert}    ${cert_file}
    END

Verify Invalid Path
    [Setup]    Setup With Self-Signed Certificate
    ${result}=    Execute Command    ls /invalid/path/*.pem    ignore_exit_code=True    stderr=True    stdout=True
    ${stdout}=    Set Variable    ${result}[1]
    Should Contain    ${stdout}    No such file or directory


*** Keywords ***
Setup With Self-Signed Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --cert-method selfsigned

Setup Without Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --install --no-bootstrap --no-connect

Should Contain Any Line
    [Arguments]    ${text}    ${cert_file}
    ${cert_present}=    Run Keyword And Return Status    Should Contain    ${text}    -----BEGIN CERTIFICATE-----
    ${key_present}=    Run Keyword And Return Status    Should Contain    ${text}    -----BEGIN PRIVATE KEY-----
    IF    ${cert_present}    Log    Certificate found in ${cert_file}
    IF    ${key_present}    Log    Private key found in ${cert_file}
    IF    not ${cert_present} and not ${key_present}
        Fail    The file ${cert_file} does not contain a valid certificate or private key.
    END

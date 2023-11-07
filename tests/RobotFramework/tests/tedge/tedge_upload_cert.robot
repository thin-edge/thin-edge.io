*** Settings ***
Documentation    Run certificate upload test and fails to upload the cert because there is no root certificate directory,
...              Then check the negative response in stderr              

Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:mqtt    theme:c8y
Suite Setup            Setup
Suite Teardown         Get Logs

*** Test Cases ***

tedge cert upload c8y command fails
    Execute Command    tedge config set c8y.root_cert_path /etc/ssl/certs_test    
    ${output}=    Execute Command    sudo env C8YPASS\='password' tedge cert upload c8y --user testuser    ignore_exit_code=${True}    stdout=${False}    stderr=${True}
    Execute Command    tedge config unset c8y.root_cert_path
    Should Contain    ${output}    Root certificate path /etc/ssl/certs_test does not exist  
    
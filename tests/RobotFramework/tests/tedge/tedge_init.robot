*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:cli


*** Test Cases ***
call tedge init
    Execute Command    tedge config set logs.path /var/local-logs/tedge
    Execute Command    sudo tedge init --user tedge --group tedge
    ${output}=    Execute Command    ls -ld /var/local-logs
    Should Contain    ${output}    root root
    ${output}=    Execute Command    ls -ld /var/local-logs/tedge
    Should Contain    ${output}    tedge tedge

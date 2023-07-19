*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:configuration
Suite Setup            Custom Setup
Suite Teardown         Custom Teardown

*** Test Cases ***

Check existence of init directories
    [Documentation]    During Custom Setup these folders were removed
    ...    this test step is confirming the deletion of the folders
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/mosquitto-conf
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/operations
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/plugins
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge/device-certs
    ThinEdgeIO.Directory Should Not Exist    /var/tedge
    ThinEdgeIO.Directory Should Not Exist    /var/log/tedge

Tedge init and check creation of folders
    [Documentation]    Running tedge init should create these folders
    ...    this test step is confirming the creation of the folders
    Execute Command    sudo tedge init
    ThinEdgeIO.Directory Should Exist    /etc/tedge
    ThinEdgeIO.Directory Should Exist    /etc/tedge/mosquitto-conf
    ThinEdgeIO.Directory Should Exist    /etc/tedge/operations
    ThinEdgeIO.Directory Should Exist    /etc/tedge/plugins
    ThinEdgeIO.Directory Should Exist    /etc/tedge/device-certs
    ThinEdgeIO.Directory Should Exist    /var/tedge
    ThinEdgeIO.Directory Should Exist    /var/log/tedge

Check ownership of the folders
    [Documentation]    Running tedge init has created the folders assigning default user/group
    ...    this test step is confirming the default values for user/group
    Check Owner of Directory    /etc/tedge    tedge:root
    Check Owner of Directory    /etc/tedge/mosquitto-conf    mosquitto:mosquitto
    Check Owner of Directory    /etc/tedge/operations    tedge:tedge
    Check Owner of Directory    /etc/tedge/plugins    tedge:tedge
    Check Owner of Directory    /etc/tedge/device-certs    mosquitto:mosquitto
    Check Owner of Directory    /var/tedge    tedge:tedge
    Check Owner of Directory    /var/log/tedge    tedge:tedge

Change user/group and check the change
    [Documentation]    Running tedge init --user <user> --group <group>  is setting custom user/group
    ...    this test step is confirming the custom values for user/group
    Execute Command    sudo tedge init --user petertest --group petertest
    Check Owner of Directory    /etc/tedge    petertest:root
    Check Owner of Directory    /etc/tedge/mosquitto-conf    mosquitto:mosquitto
    Check Owner of Directory    /etc/tedge/operations    petertest:petertest
    Check Owner of Directory    /etc/tedge/plugins    petertest:petertest
    Check Owner of Directory    /etc/tedge/device-certs    mosquitto:mosquitto
    Check Owner of Directory    /var/tedge    petertest:petertest
    Check Owner of Directory    /var/log/tedge    petertest:petertest

Tedge init and check if default values are restored
    [Documentation]    Running tedge init after setting custom user/group should restore the default user/group
    ...    this test step is confirming the default values for user/group
    Execute Command    sudo tedge init
    Check Owner of Directory    /etc/tedge    tedge:root
    Check Owner of Directory    /etc/tedge/mosquitto-conf    mosquitto:mosquitto
    Check Owner of Directory    /etc/tedge/operations    tedge:tedge
    Check Owner of Directory    /etc/tedge/plugins    tedge:tedge
    Check Owner of Directory    /etc/tedge/device-certs    mosquitto:mosquitto
    Check Owner of Directory    /var/tedge    tedge:tedge
    Check Owner of Directory    /var/log/tedge    tedge:tedge

*** Keywords ***

Custom Setup
    Setup
    Execute Command    sudo rm -rf /etc/tedge
    Execute Command    sudo rm -rf /var/tedge
    Execute Command    sudo rm -rf /var/log/tedge

Custom Teardown
    Get Logs

Check Owner of Directory
    [Arguments]    ${directory_path}    ${expected_owner}
    ${output}    Execute Command    ls -ld ${directory_path} | awk '{print $3 ":" $4}'
    Should Match Regexp    ${output}    \s*${expected_owner}\s*

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
    ${etc_tedge}   Execute Command    ls -ld /etc/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge}    \s*tedge:root\s*
    ${etc_tedge_mosq}   Execute Command    ls -ld /etc/tedge/mosquitto-conf | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_mosq}    \s*mosquitto:mosquitto\s*
    ${etc_tedge_operat}   Execute Command    ls -ld /etc/tedge/operations | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_operat}    \s*tedge:tedge\s*    
    ${etc_tedge_plug}   Execute Command    ls -ld /etc/tedge/plugins | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_plug}    \s*tedge:tedge\s*    
    ${etc_tedge_devcert}   Execute Command    ls -ld /etc/tedge/device-certs | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_devcert}    \s*mosquitto:mosquitto\s*
    ${var_tedge}   Execute Command    ls -ld /var/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${var_tedge}    \s*tedge:tedge\s*
    ${var_log_tedge}   Execute Command    ls -ld /var/log/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${var_log_tedge}    \s*tedge:tedge\s*

Change user/group and check the change
    [Documentation]    Running tedge init --user <user> --group <group>  is setting custom user/group
    ...    this test step is confirming the custom values for user/group
    Execute Command    sudo tedge init --user petertest --group petertest
    ${etc_tedge}   Execute Command    ls -ld /etc/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge}    \s*petertest:root\s*
    ${etc_tedge_mosq}   Execute Command    ls -ld /etc/tedge/mosquitto-conf | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_mosq}    \s*mosquitto:mosquitto\s*
    ${etc_tedge_operat}   Execute Command    ls -ld /etc/tedge/operations | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_operat}    \s*petertest:petertest\s*  
    ${etc_tedge_plug}   Execute Command    ls -ld /etc/tedge/plugins | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_plug}    \s*petertest:petertest\s*   
    ${etc_tedge_devcert}   Execute Command    ls -ld /etc/tedge/device-certs | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_devcert}    \s*mosquitto:mosquitto\s*
    ${var_tedge}   Execute Command    ls -ld /var/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${var_tedge}    \s*petertest:petertest\s*
    ${var_log_tedge}   Execute Command    ls -ld /var/log/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${var_log_tedge}    \s*petertest:petertest\s*

Tedge init and check if default values are restored
    [Documentation]    Running tedge init after setting custom user/group should restore the default user/group
    ...    this test step is confirming the default values for user/group
    Execute Command    sudo tedge init
    ${etc_tedge}   Execute Command    ls -ld /etc/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge}    \s*tedge:root\s*
    ${etc_tedge_mosq}   Execute Command    ls -ld /etc/tedge/mosquitto-conf | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_mosq}    \s*mosquitto:mosquitto\s*
    ${etc_tedge_operat}   Execute Command    ls -ld /etc/tedge/operations | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_operat}    \s*tedge:tedge\s*    
    ${etc_tedge_plug}   Execute Command    ls -ld /etc/tedge/plugins | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_plug}    \s*tedge:tedge\s*    
    ${etc_tedge_devcert}   Execute Command    ls -ld /etc/tedge/device-certs | awk '{print $3 ":" $4}'
    Should Match Regexp    ${etc_tedge_devcert}    \s*mosquitto:mosquitto\s*
    ${var_tedge}   Execute Command    ls -ld /var/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${var_tedge}    \s*tedge:tedge\s*
    ${var_log_tedge}   Execute Command    ls -ld /var/log/tedge | awk '{print $3 ":" $4}'
    Should Match Regexp    ${var_log_tedge}    \s*tedge:tedge\s*


*** Keywords ***

Custom Setup
    Setup
    Execute Command    sudo rm -rf /etc/tedge
    Execute Command    sudo rm -rf /etc/tedge/mosquitto-conf
    Execute Command    sudo rm -rf /etc/tedge/operations
    Execute Command    sudo rm -rf /etc/tedge/plugins
    Execute Command    sudo rm -rf /etc/tedge/device-certs
    Execute Command    sudo rm -rf /var/tedge
    Execute Command    sudo rm -rf /var/log/tedge

Custom Teardown
    Get Logs

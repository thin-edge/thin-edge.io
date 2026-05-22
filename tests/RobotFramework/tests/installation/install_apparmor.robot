*** Settings ***
Documentation       AppArmor can't be run inside a container, so these tests are only
...                 smoke tests to try to reflect the environment as close a possible
...                 without testing it's full compatibility with AppArmor

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:installation


*** Test Cases ***
AppArmor local mosquitto config is created on fresh install
    [Documentation]    When /etc/apparmor.d/local/mosquitto does not exist before installation,
    ...    postinst creates it with the tedge block, and purging tedge removes the file entirely.
    Mosquitto AppArmor Config Should Not Exist

    Install Tedge    external_id=${DEVICE_SN}

    Mosquitto AppArmor Config Should Exist

    Execute Command    cmd=apt-get purge -y tedge
    Mosquitto AppArmor Config Should Not Exist

AppArmor local mosquitto config is appended to existing file on install
    [Documentation]    When /etc/apparmor.d/local/mosquitto already contains custom rules,
    ...    postinst appends the tedge guarded block without overwriting existing content.
    ...    Purging tedge removes only the tedge block and leaves the rest of the file intact.
    Execute Command
    ...    cmd=mkdir -p /etc/apparmor.d/local && printf '# Custom site rule\n/tmp/ r,\n' > /etc/apparmor.d/local/mosquitto

    Install Tedge    external_id=${DEVICE_SN}

    # Both the pre-existing content and tedge's guarded block should be present
    Mosquitto AppArmor Config Should Exist
    Execute Command    cmd=grep -q '# Custom site rule' /etc/apparmor.d/local/mosquitto

    # After purge, tedge block is gone but file and pre-existing content remain
    Execute Command    cmd=apt-get purge -y tedge
    File Should Exist    path=/etc/apparmor.d/local/mosquitto
    Execute Command    cmd=grep -q 'BEGIN tedge' /etc/apparmor.d/local/mosquitto    exp_exit_code=1
    Execute Command    cmd=grep -q '# Custom site rule' /etc/apparmor.d/local/mosquitto


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN

Install Tedge
    [Arguments]    ${external_id}
    ${bootstrap_cmd}=    ThinEdgeIO.Get Bootstrap Command
    Execute Command    cmd=${bootstrap_cmd}
    Register Device With Cumulocity CA    external_id=${external_id}
    Execute Command    cmd=tedge connect c8y

Mosquitto AppArmor Config Should Exist
    Execute Command    cmd=grep -q 'BEGIN tedge' /etc/apparmor.d/local/mosquitto

Mosquitto AppArmor Config Should Not Exist
    File Should Not Exist    path=/etc/apparmor.d/local/mosquitto

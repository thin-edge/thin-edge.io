*** Settings ***
Resource            ../resources/common.resource
Library             ThinEdgeIO

Documentation
...    Tests for tedge-write need to be done in RobotFramework because we need to run as a `tedge`
...    user, and a sudoers entry that allows `tedge` to run `sudo tedge-write` without password
...    needs to be present.

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           adapter:docker    theme:tedge-write


*** Test Cases ***
Creates a destination file if it doesn't exist
    [Template]    Creates a destination file if it doesn't exist
    0022
    0777

Changes permissions if a destination doesn't exist
    [Template]    Changes permissions if a destination doesn't exist
    0022
    0777

Preserves permissions if a destination exists
    [Template]    Preserves permissions if a destination exists
    0022
    0777

*** Keywords ***
Creates a destination file if it doesn't exist
    [Arguments]          ${umask}

    Execute Command      umask ${umask}

    ${dest_filename}=    Execute Command    mktemp --dry-run
    Execute Command      echo abc | sudo tedge-write ${dest_filename}    strip=True

    Execute Command      ls ${dest_filename}
    ${user_owner}=       Execute Command    stat -c '%U' ${dest_filename}    strip=True
    Should Be Equal      ${user_owner}    root
    ${group_owner}=      Execute Command    stat -c '%G' ${dest_filename}    strip=True
    Should Be Equal      ${group_owner}    root


Changes permissions if a destination doesn't exist
    [Arguments]          ${umask}

    Execute Command      umask ${umask}

    ${dest_filename}=    Execute Command    mktemp --dry-run    strip=True
    Execute Command      echo abc | sudo tedge-write ${dest_filename} --user tedge --group tedge --mode 700

    Execute Command      ls ${dest_filename}
    ${user_owner}=       Execute Command    stat -c '%U' ${dest_filename}    strip=True
    Should Be Equal      ${user_owner}    tedge
    ${group_owner}=      Execute Command    stat -c '%G' ${dest_filename}    strip=True
    Should Be Equal      ${group_owner}    tedge
    ${mode}=      Execute Command    stat -c '%a' ${dest_filename}    strip=True
    Should Be Equal      ${mode}    700

Preserves permissions if a destination exists
    [Arguments]          ${umask}

    Execute Command      umask ${umask}

    ${dest_filename}=    Execute Command    mktemp    strip=True
    Execute Command      chown tedge:tedge ${dest_filename}
    Execute Command      chmod 666 ${dest_filename}

    Execute Command      echo abc | sudo tedge-write ${dest_filename} --user root --group root --mode 700
    Execute Command      ls ${dest_filename}
    ${user_owner}=       Execute Command    stat -c '%U' ${dest_filename}    strip=True
    Should Be Equal      ${user_owner}    tedge
    ${group_owner}=      Execute Command    stat -c '%G' ${dest_filename}    strip=True
    Should Be Equal      ${group_owner}    tedge
    ${mode}=      Execute Command    stat -c '%a' ${dest_filename}    strip=True
    Should Be Equal      ${mode}    666

Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect
    Execute Command    sudo --user\=tedge bash


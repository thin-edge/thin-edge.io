*** Settings ***
Documentation
...                 This suite aims to check the correctness of tedge-write component which is used for privilege
...                 elevation to allow tedge processes running under `tedge` user to write to configuration files
...                 on the system tedge normally doesn't have permissions for.
...
...                 tedge-write must:
...                 - write to files `tedge` have no permissions to (privilege elevation happens)
...                 - preserve permissions for existing files and apply new permissions for files created
...                 - handle permissions correctly on non-standard umasks
...                 - not leave temporary files
...
...                 Tests for tedge-write need to be done in RobotFramework because we need to run as a `tedge`
...                 user, and a sudoers entry that allows `tedge` to run `sudo tedge-write` without password
...                 needs to be present. Additionally, we need to ensure `tedge-write` works well on system where
...                 the default umask is different.
...
...                 Additionally, apart from the component itself, we need to test features that use it directly,
...                 like config_update.

Resource            ../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs
Test Teardown       Remove temporary test directories

Test Tags           adapter:docker    theme:tedge-write


*** Variables ***
${REGULAR_USER}     user


*** Test Cases ***
Sudo elevates permissions when tedge-write writes to a root-owned file
    ${dest_file}=    Create a temporary file
    Make file inaccessible to regular users    ${dest_file}

    Write content to file using sudo tedge-write as user    tedge    ${dest_file}

    There should be no leftover temporary files    ${dest_file}

Sudo doesn't elevate permissions when tedge-write is run by another user
    ${dest_file}=    Create a temporary file
    Make file inaccessible to regular users    ${dest_file}

    Write content to file using sudo tedge-write as user
    ...    ${REGULAR_USER}    ${dest_file}    exp_exit_code=!0

    There should be no leftover temporary files    ${dest_file}

Creates a destination file if it doesn't exist
    [Template]    Creates a destination file if it doesn't exist
    0000
    0022
    0777

Changes permissions if a destination doesn't exist
    [Template]    Changes permissions if a destination doesn't exist
    0000
    0022
    0777

Preserves permissions if a destination exists
    [Template]    Preserves permissions if a destination exists
    0000
    0022
    0777


*** Keywords ***
Creates a destination file if it doesn't exist
    [Arguments]    ${umask}

    ${dest_filename}=    Create a temporary file    dry_run=${True}
    # `tedge-write` is prepended with `umask ${umask}` because new umask only applies to the current
    # process, and `Execute Command` starts a new shell process every time
    Execute Command as user    tedge
    ...    umask ${umask} && echo abc | sudo tedge-write ${dest_filename}    strip=True

    Path Should Have Permissions    ${dest_filename}    owner_group=root:root

    There should be no leftover temporary files    ${dest_filename}

Changes permissions if a destination doesn't exist
    [Arguments]    ${umask}

    ${dest_filename}=    Create a temporary file    dry_run=${True}
    Execute Command as user    tedge
    ...    umask ${umask} && echo abc | sudo tedge-write ${dest_filename} --user tedge --group tedge --mode 700

    Path Should Have Permissions    ${dest_filename}    mode=700    owner_group=tedge:tedge

    There should be no leftover temporary files    ${dest_filename}

Preserves permissions if a destination exists
    [Arguments]    ${umask}

    ${dest_file}=    Create a temporary file
    Execute Command    chown tedge:tedge ${dest_file}
    Execute Command    chmod 666 ${dest_file}

    Execute Command as user    tedge
    ...    umask ${umask} && echo abc | sudo tedge-write ${dest_file} --user root --group root --mode 700

    Path Should Have Permissions    ${dest_file}    mode=666    owner_group=tedge:tedge

    There should be no leftover temporary files    ${dest_file}

Custom Setup
    Setup    skip_bootstrap=True
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect

    Create a regular user

Create a temporary file
    [Documentation]
    ...    Creates a temporary directory and creates a temporary file in that directory. This is so we can check that
    ...    after writing there are no leftover temporary files in the destination file's parent directory.
    [Arguments]    ${dry_run}=${False}
    ${dir}=    Execute Command    mktemp --tmpdir\=/tmp --directory    strip=True
    ${path}=    Execute Command    mktemp --tmpdir\=${dir} ${{'--dry-run' if ${dry_run} is True else ''}}    strip=True
    RETURN    ${path}

Remove temporary test directories
    [Documentation]    Remove test directories created in _Create a temporary file_ keyword.
    Execute Command    rm -r /tmp/tmp.*

Make file inaccessible to regular users
    [Arguments]    ${file}
    Execute Command    chown root:root ${file}
    Execute Command    chmod 600 ${file}

Write content to file using sudo tedge-write as user
    [Arguments]    ${user}    ${file}    &{named}
    Execute Command as user    ${user}    echo new content | sudo tedge-write ${file}    &{named}

Create a regular user
    Execute Command    sudo useradd -s /bin/bash ${REGULAR_USER}

Execute Command as user
    [Arguments]    ${user}    ${command}    &{named}
    Execute Command    sudo -n --user\=${user} bash -c '${command}'    &{named}

There should be no leftover temporary files
    [Documentation]    Assuming ${dest_file} was the only file in a temporary directory that was written to by
    ...    tedge-agent, check that there are no other files in the directory after using tedge-write.
    [Arguments]    ${dest_file}

    ${num_files}=    Execute Command    ls $(dirname ${dest_file}) | wc -l    strip=True
    Should Be Equal    ${num_files}    1

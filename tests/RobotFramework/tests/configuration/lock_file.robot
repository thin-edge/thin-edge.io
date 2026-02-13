*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Test Teardown       Get Logs

Test Tags           theme:configuration


*** Test Cases ***
Check lock file existence in default folder
    [Documentation]    When deploying thin-edge components in single-process containers,
    ...    the lock file mechanism used by some components (e.g. tedge-agent, tedge-mapper)
    ...    do not makes sense as the container filesystem is isolated.
    ...    Having the lock file system just adds unnecessary dependencies on the /run/lock folder.
    File Should Exist    /run/lock/tedge-agent.lock
    File Should Exist    /run/lock/tedge-mapper-c8y.lock

Check PID number in lock file
    [Documentation]    Include the pid inside the existing lock files under /run/lock/
    ${pid_agent1}=    Service Should Be Running    tedge-agent
    ${pid_mapper1}=    Service Should Be Running    tedge-mapper-c8y
    File Contents Should Be Equal    /run/lock/tedge-agent.lock    ${pid_agent1}
    File Contents Should Be Equal    /run/lock/tedge-mapper-c8y.lock    ${pid_mapper1}

Check PID number in lock file after restarting the services
    [Documentation]    Include the new pid generated after service restart
    ...    inside the existing lock files under /run/lock/
    Stop/Start Service    tedge-agent
    Stop/Start Service    tedge-mapper-c8y
    ${pid_agent2}=    Service Should Be Running    tedge-agent
    ${pid_mapper2}=    Service Should Be Running    tedge-mapper-c8y
    File Contents Should Be Equal    /run/lock/tedge-agent.lock    ${pid_agent2}
    File Contents Should Be Equal    /run/lock/tedge-mapper-c8y.lock    ${pid_mapper2}

Check starting same service twice
    [Documentation]    This step is checking if same service can be started twice,
    ...    expected is that this should not be the case
    Execute Command    sudo -u tedge tedge-agent    exp_exit_code=!0
    Execute Command    sudo -u tedge tedge-mapper c8y    exp_exit_code=!0

Switch off lock file creation
    [Documentation]    Add a new configuration option under the '[run]'' section to turn off the lock file generation logic.
    ...    '[run]'
    ...    'lock_files = false'
    ...    Having this configuration setting allows the user to set it using a common environment
    ...    setting when running the components in individual containers.
    # Stop the running services
    Stop Service    tedge-mapper-c8y
    Stop Service    tedge-agent
    # Remove the existing lock files (if they exist)
    Execute Command    sudo rm -f /run/lock/ted*
    Execute Command    sudo tedge config set run.lock_files false
    # Restart the stopped services
    Start Service    tedge-mapper-c8y
    Start Service    tedge-agent
    # Check that no lock file is created
    File Should Not Exist    /run/lock/tedge-agent.lock
    File Should Not Exist    /run/lock/tedge-mapper-c8y.lock


*** Keywords ***
Stop/Start Service
    [Arguments]    ${name}
    # Note: Use stop/start service rather than restart as 'systemctl restart <service>'
    # does not wait for all file descriptors to be flushed before returning, https://man7.org/linux/man-pages/man1/systemctl.1.html
    # See https://github.com/thin-edge/thin-edge.io/issues/2087 for more details
    Stop Service    ${name}
    Start Service    ${name}

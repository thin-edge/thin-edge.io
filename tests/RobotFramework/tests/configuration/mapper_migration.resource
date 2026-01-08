*** Settings ***
Documentation       Shared keywords for mapper config migration tests

Library             String


*** Keywords ***
Migrate Cloud Configs
    [Documentation]    Execute the migration command for all mappers
    Execute Command    tedge config upgrade

Verify Mapper Config File Exists
    [Documentation]    Verify that the mapper config file exists for the given cloud and profile
    [Arguments]    ${cloud}    ${profile}=${NONE}
    ${path}=    Get Expected Mapper Config Path    ${cloud}    ${profile}
    Execute Command    test -f ${path}

Verify Mapper Config File Contains
    [Documentation]    Verify that the mapper config file contains the expected key-value pair
    [Arguments]    ${cloud}    ${key}    ${value}    ${profile}=${NONE}
    ${path}=    Get Expected Mapper Config Path    ${cloud}    ${profile}
    ${content}=    Execute Command    cat ${path}
    Should Contain    ${content}    ${key} = "${value}"

Verify Mapper Config File Contains Pattern
    [Documentation]    Verify that the mapper config file matches a pattern (for non-quoted values)
    [Arguments]    ${cloud}    ${pattern}    ${profile}=${NONE}
    ${path}=    Get Expected Mapper Config Path    ${cloud}    ${profile}
    ${content}=    Execute Command    cat ${path}
    Should Contain    ${content}    ${pattern}

Verify Tedge Toml Contains Section
    [Documentation]    Verify that tedge.toml contains the specified section
    [Arguments]    ${section}
    ${content}=    Execute Command    cat /etc/tedge/tedge.toml
    Should Contain    ${content}    [${section}]

Verify Tedge Toml Does Not Contain Section
    [Documentation]    Verify that tedge.toml no longer contains the specified section
    [Arguments]    ${section}
    ${content}=    Execute Command    cat /etc/tedge/tedge.toml
    Should Not Contain    ${content}    [${section}]

Verify Tedge Toml Contains Pattern
    [Documentation]    Verify that tedge.toml contains a specific pattern
    [Arguments]    ${pattern}
    ${content}=    Execute Command    cat /etc/tedge/tedge.toml
    Should Contain    ${content}    ${pattern}

Get Expected Mapper Config Path
    [Documentation]    Calculate the expected path for mapper config based on cloud and profile
    [Arguments]    ${cloud}    ${profile}=${NONE}
    IF    "${profile}" == "${NONE}"
        ${path}=    Set Variable    /etc/tedge/mappers/${cloud}.toml
    ELSE
        ${path}=    Set Variable    /etc/tedge/mappers/${cloud}.d/${profile}.toml
    END
    RETURN    ${path}

Verify Config Command Blocked
    [Documentation]    Verify that a config command is blocked with the expected error message
    [Arguments]    ${command}    ${expected_error_pattern}
    ${result}=    Execute Command    ${command}    exp_exit_code=!0    stderr=${True}    stdout=${True}
    Should Contain    ${result}    ${expected_error_pattern}

Setup Test Config
    [Documentation]    Setup a test configuration for the given cloud and URL
    [Arguments]    ${cloud}    ${url}    ${profile}=${NONE}
    IF    "${profile}" == "${NONE}"
        ${key}=    Set Variable    ${cloud}.url
    ELSE
        ${key}=    Set Variable    ${cloud}.profiles.${profile}.url
    END
    Execute Command    sudo tedge config set ${key} ${url}

Parse Connect Output For Config Location
    [Documentation]    Extract the mapper configuration file line from tedge connect output
    [Arguments]    ${output}
    @{lines}=    Split To Lines    ${output}
    FOR    ${line}    IN    @{lines}
        ${is_mapper_line}=    Run Keyword And Return Status    Should Contain    ${line}    mapper configuration file
        IF    ${is_mapper_line}
            ${trimmed}=    Strip String    ${line}
            RETURN    ${trimmed}
        END
    END
    Fail    Could not find 'mapper configuration file' line in connect output

Clean Migration State
    [Documentation]    Clean all migration state to start fresh
    Execute Command    sudo rm -rf /etc/tedge/mappers
    Execute Command    sudo rm -f /etc/tedge/tedge.toml
    Execute Command    sudo tedge init

Verify Mapper Uses Config
    [Documentation]    Verify that the mapper is actually using the configured URL/host
    [Arguments]    ${cloud}    ${expected_host}
    # Start mapper and check it's using the expected config
    Execute Command    sudo systemctl restart tedge-mapper-${cloud}
    Sleep    5s
    Service Health Status Should Be Up    tedge-mapper-${cloud}
    # Check logs for evidence of the expected host
    ${logs}=    Execute Command    sudo journalctl -u tedge-mapper-${cloud} --no-pager -n 100
    Should Contain    ${logs}    ${expected_host}

Migration Suite Setup
    [Documentation]    Common suite setup for migration tests
    ${DEVICE_SN}=    Setup    skip_bootstrap=${False}
    Set Suite Variable    ${DEVICE_SN}
    # Backup existing config
    Execute Command    mkdir -p /tmp/tedge-backup
    Execute Command    sudo cp -r /etc/tedge/* /tmp/tedge-backup/ 2>/dev/null || true

Migration Suite Teardown
    [Documentation]    Common suite teardown for migration tests
    Get Suite Logs
    # Restore original config
    Execute Command    sudo rm -rf /etc/tedge/*
    Execute Command    sudo cp -r /tmp/tedge-backup/* /etc/tedge/ 2>/dev/null || true
    Execute Command    rm -rf /tmp/tedge-backup

Prepare Clean Environment
    [Documentation]    Prepare a clean environment for testing
    Execute Command    sudo rm -rf /etc/tedge/mappers
    # Ensure tedge is initialized
    Execute Command    sudo tedge init

Test Cleanup
    [Documentation]    Clean up after each test
    Get Logs
    Execute Command    sudo systemctl stop tedge-mapper-* 2>/dev/null || true

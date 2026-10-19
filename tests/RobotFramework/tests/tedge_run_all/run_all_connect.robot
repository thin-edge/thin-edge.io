*** Settings ***
Documentation       `tedge connect`, `tedge reconnect` and `tedge disconnect` detect when the
...                 components run inside `tedge run all` (an active init service called `tedge`)
...                 and then act on that service instead of on the per-component services,
...                 which are masked in these tests to prove they are not used.
...                 See https://github.com/thin-edge/thin-edge.io/issues/4365

Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Register Device

Test Tags           theme:supervisor    theme:c8y    theme:cli


*** Test Cases ***
Reconnect uses the per-component services when tedge run all is not used
    ${output}=    Execute Command    cmd=tedge reconnect c8y 2>&1
    Should Contain    ${output}    Enabling tedge-mapper-c8y
    Should Contain    ${output}    Enabling tedge-agent
    Should Not Contain    ${output}    tedge run all
    Should Not Contain    ${output}    ✗
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-agent
    [Teardown]    Get Logs

Reconnect restarts the tedge service when tedge run all is used
    [Setup]    Use Tedge Run All Service
    ${pid_before}=    Get Service PID    tedge

    ${output}=    Execute Command    cmd=tedge reconnect c8y 2>&1
    Should Contain    ${output}    Restarting tedge (tedge run all)
    Should Not Contain    ${output}    Enabling tedge-mapper-c8y
    Should Not Contain    ${output}    Enabling tedge-agent
    Should Not Contain    ${output}    ✗

    # The whole process was restarted, and is enabled to come back after a reboot
    ${pid_after}=    Get Service PID    tedge
    Should Not Be Equal    ${pid_before}    ${pid_after}
    Execute Command    systemctl is-enabled tedge

    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y
    [Teardown]    Restore Per-Component Services

Reconnect a cloud profile when tedge run all is used
    [Setup]    Use Tedge Run All Service With Cloud Profile
    ${pid_before}=    Get Service PID    tedge

    ${output}=    Execute Command    cmd=tedge reconnect c8y --profile other 2>&1
    Should Contain    ${output}    Restarting tedge (tedge run all)
    Should Not Contain    ${output}    Enabling tedge-mapper-c8y@other
    Should Not Contain    ${output}    Enabling tedge-agent
    Should Not Contain    ${output}    ✗

    # The restarted process runs the mapper of the `other` profile, the only c8y mapper configured.
    # A c8y mapper reports its health as `tedge-mapper-c8y` whatever its profile.
    ${pid_after}=    Get Service PID    tedge
    Should Not Be Equal    ${pid_before}    ${pid_after}
    ${health}=    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y
    Should Be Equal As Integers    ${health["pid"]}    ${pid_after}
    [Teardown]    Restore Per-Component Services Without Cloud Profile

Disconnect leaves the tedge service alone when tedge run all is used
    [Setup]    Use Tedge Run All Service
    ${pid_before}=    Get Service PID    tedge

    ${output}=    Execute Command    cmd=tedge disconnect c8y 2>&1
    Should Contain    ${output}    tedge-mapper-c8y runs inside tedge (tedge run all); not disabling it
    Should Not Contain    ${output}    Disabling tedge-mapper-c8y
    Should Not Contain    ${output}    ✗

    ${pid_after}=    Get Service PID    tedge
    Should Be Equal    ${pid_before}    ${pid_after}
    Execute Command    systemctl is-active tedge
    [Teardown]    Restore Per-Component Services


*** Keywords ***
Register Device
    ${DEVICE_SN}=    Setup
    Set Suite Variable    ${DEVICE_SN}
    Device Should Exist    ${DEVICE_SN}

Use Tedge Run All Service
    # Hand the components over to a `tedge` service running `tedge run all`, as a user would.
    # Masking the per-component services makes any attempt to start them fail, as in the issue.
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge.service    /etc/systemd/system/tedge.service
    Execute Command    systemctl stop tedge-agent tedge-mapper-c8y
    Execute Command    systemctl mask tedge-agent tedge-mapper-c8y tedge-mapper-c8y@other
    Execute Command    systemctl daemon-reload && systemctl start tedge
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

Use Tedge Run All Service With Cloud Profile
    # Move the default c8y connection to the `other` profile,
    # so the profile reuses the device certificate without a second connection fighting over it
    Use Tedge Run All Service
    Execute Command    tedge disconnect c8y
    Execute Command    systemctl stop tedge
    Execute Command    mv /etc/tedge/mappers/c8y /etc/tedge/mappers/c8y.other
    Execute Command    systemctl start tedge

Restore Per-Component Services
    Get Logs
    Execute Command    systemctl disable --now tedge    ignore_exit_code=${True}
    Execute Command    rm -f /etc/systemd/system/tedge.service
    Execute Command    systemctl unmask tedge-agent tedge-mapper-c8y tedge-mapper-c8y@other
    Execute Command    systemctl daemon-reload
    Execute Command    tedge reconnect c8y

Restore Per-Component Services Without Cloud Profile
    Execute Command    tedge disconnect c8y --profile other    ignore_exit_code=${True}
    Execute Command    systemctl stop tedge    ignore_exit_code=${True}
    Execute Command    mv /etc/tedge/mappers/c8y.other /etc/tedge/mappers/c8y
    Restore Per-Component Services

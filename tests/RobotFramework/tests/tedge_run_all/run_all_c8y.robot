*** Settings ***
Documentation       Smoke tests for the single-process supervisor (`tedge run all`).
...                 Runs the agent and the c8y mapper together inside one process and
...                 checks both come up healthy and the device keeps talking to
...                 Cumulocity, that the supervisor's locks make it mutually exclusive
...                 with the standalone components, that SIGUSR1 really restarts
...                 the mapper (proven via the mapper's health `time`, which is refreshed
...                 on every restart, while its pid stays put — only the supervised task
...                 is rebuilt, not the process), and that an update requiring an agent
...                 restart exits the whole process for the service manager to restart it.
...
...                 The device is registered once for the suite, but each test gets a
...                 freshly started supervisor and tears it down again afterwards, so the
...                 tests share no supervisor state and any one of them can be run on its
...                 own.

Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Register Device
Test Setup          Start Supervisor
Test Teardown       Stop Supervisor

Test Tags           theme:supervisor    theme:c8y


*** Test Cases ***
Run the agent and c8y mapper under a single supervisor
    # Both components run inside the one process and report healthy.
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    # And the supervised mapper keeps the device talking to Cumulocity.
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-bridge-c8y
    External Identity Should Exist    ${DEVICE_SN}
    Cumulocity.Should Have Services    name=tedge-mapper-c8y    service_type=service    status=up

The supervisor is mutually exclusive with the standalone components
    # Wait until the supervised mapper is up so it definitely holds the per-component
    # locks before we probe them.
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    # While `tedge run all c8y` holds the per-component locks, a standalone mapper
    # must refuse to start rather than clobber the supervised one. The standalone
    # mapper fails fast on the lock, so this does not block.
    Execute Command    tedge-mapper c8y    exp_exit_code=!0

A config update restarting the agent restarts the whole process
    # A configuration update whose config type declares `service = "tedge-agent"`
    # makes the agent request a restart of itself. Such a restart (like the one after
    # a self-update) only takes effect by re-executing the binary, so the supervisor
    # must exit the whole process and let the service manager start it again; the
    # resumed operation then completes on the restarted agent.
    [Setup]    Start Supervisor With Restart On Failure
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    # Declare a config type whose update restarts tedge-agent.
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/tedge-configuration-plugin.toml
    ...    /etc/tedge/plugins/tedge-configuration-plugin.toml
    Should Contain Supported Configuration Types    dummy-restart

    ${pid_before}=    Get Service PID    tedge-run-all

    ${config_url}=    Cumulocity.Create Inventory Binary
    ...    dummy-restart
    ...    dummy-restart
    ...    file=${CURDIR}/dummy-restart.toml
    ${operation}=    Cumulocity.Set Configuration    dummy-restart    url=${config_url}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120

    # The whole process was re-executed by the service manager, not rebuilt in-process.
    ${pid_after}=    Get Service PID    tedge-run-all
    Should Not Be Equal    ${pid_before}    ${pid_after}

    # Both components come back up in the restarted process.
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y
    [Teardown]    Stop Supervisor And Remove Config Type

SIGUSR1 restarts the mapper
    # The mapper publishes a health `up` message every time it (re)starts, stamped
    # with a fresh `time`. Its `pid`, on the other hand, is the supervisor process id,
    # which does not change across a restart. We use both to prove SIGUSR1 rebuilt the
    # mapper task: the `time` must advance while the `pid` stays the same.

    # Baseline: capture the mapper's current health `up` message.
    ${before}=    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    # Restart the mapper. The agent is deliberately not targeted by SIGUSR1.
    Execute Command    cmd=systemctl kill --signal=SIGUSR1 tedge-run-all.service

    # The rebuilt mapper republishes a health `up` with a strictly newer `time`, while
    # the `pid` is unchanged — only the supervised task was restarted, not the process.
    Wait Until Keyword Succeeds
    ...    60s    2s    Mapper Restarted Since    ${before}

    # The agent was never targeted by SIGUSR1 and stayed up throughout.
    Service Health Status Should Be Up    tedge-agent


*** Keywords ***
Register Device
    # Suite-level, so the (relatively expensive) Cumulocity device registration only
    # happens once. Self-signed registration keeps the device self-contained: the
    # certificate is generated and trusted locally, without depending on Cumulocity's
    # certificate-authority enrolment.
    ${DEVICE_SN}=    Setup    register_using=self-signed    connect=${False}
    Set Suite Variable    ${DEVICE_SN}

    # Establish connectivity the normal way first, to prove a working baseline and to
    # lay down the bridge configuration the supervisor's bridge reuses.
    Execute Command    tedge connect c8y

    # Hand the components over to the supervisor for good: stop the systemd-managed
    # services so their single-instance locks stay free for `tedge run all`. We never
    # start them again, so the locks remain available to every test's supervisor.
    Execute Command    systemctl stop tedge-mapper-c8y tedge-agent

Start Supervisor
    # Per-test, so every test starts from the same clean state: a freshly launched
    # supervisor on the already-registered device. `tedge run all` is a long-running
    # foreground process, so launch it as a transient systemd unit (running as the
    # tedge user, like the real services) and let it run for the duration of the test.
    # `--collect` reaps the unit when it stops, freeing the name for the next test.
    Execute Command
    ...    cmd=systemd-run --unit=tedge-run-all --collect -p User=tedge -p Group=tedge /usr/bin/tedge run all c8y

Start Supervisor With Restart On Failure
    # Like Start Supervisor, but with the restart policy the packaged service files
    # use: when a component requires a process restart (a self-update, or an update of
    # the agent's own configuration) the supervisor exits non-zero and relies on the
    # service manager to start it again.
    Execute Command
    ...    cmd=systemd-run --unit=tedge-run-all --collect -p User=tedge -p Group=tedge -p Restart=on-failure /usr/bin/tedge run all c8y

Stop Supervisor And Remove Config Type
    # Drop the config type declaration (and the file its update created) so the other
    # tests of the suite see the device exactly as the suite setup left it.
    Execute Command    rm -f /etc/tedge/plugins/tedge-configuration-plugin.toml /etc/tedge/dummy-restart.toml
    Stop Supervisor

Mapper Restarted Since
    [Arguments]    ${before}
    # A restart is observable purely on the local broker: the rebuilt mapper publishes
    # a fresh health `up` message whose `time` is strictly newer than the baseline's,
    # while its `pid` is unchanged because it is the same supervisor process throughout
    # (only the supervised mapper task is rebuilt). The pid-unchanged check is therefore
    # specific to `tedge run all`; a standalone, separately-process mapper would get a
    # new pid on restart.
    ${now}=    Service Health Status Should Be Up    tedge-mapper-c8y
    Should Be True    ${now["time"]} > ${before["time"]}
    Should Be Equal As Integers    ${now["pid"]}    ${before["pid"]}

Stop Supervisor
    # Tear the supervisor down after each test so it leaves no state behind for the
    # next one. The device registration from the suite setup is left untouched.
    Execute Command    systemctl stop tedge-run-all.service    ignore_exit_code=${True}
    Get Logs

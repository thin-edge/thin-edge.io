*** Settings ***
Documentation       Smoke tests for the single-process supervisor (`tedge run all`).
...                 Runs the agent and the c8y mapper together inside one process and
...                 checks both come up healthy and the device keeps talking to
...                 Cumulocity, that the supervisor's locks make it mutually exclusive
...                 with the standalone components, that SIGUSR1 really restarts
...                 the mapper (proven via the mapper's health `time`, which is refreshed
...                 on every restart, while its pid stays put — only the supervised task
...                 is rebuilt, not the process), that an update requiring an agent
...                 restart exits the whole process for the service manager to restart it,
...                 that a hosted service declares only the action it can carry out with no
...                 init unit of its own — the agent restarting itself — and that the
...                 supervisor can host multiple mappers (c8y + a custom flows-only mapper)
...                 simultaneously.
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

Declares only the action a hosted service can carry out
    [Documentation]    No hosted component has an init unit of its own, so an action going
    ...    through systemctl would act on a unit which is not what runs. Only the agent
    ...    restarting itself never reaches an init system, so that is all the agent declares
    ...    and the mapper declares nothing. The suite setup ran both as standalone services
    ...    first, so their declarations from that deployment are on show when the supervisor
    ...    starts: a capability is retained, and clearing them is what the hosted services do.
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    Supported Service Commands Should Be    ${AGENT_XID}    RESTART
    Supported Service Commands Should Be    ${MAPPER_XID}

Restarts tedge-agent under the supervisor
    [Documentation]    The agent restarts itself rather than asking a backend, so the action
    ...    works with no unit of its own: the supervisor exits the whole process and the service
    ...    manager starts it again, the mapper coming back with it. The operation completes once
    ...    the agent resumes from the state it persisted before stopping.
    [Setup]    Start Supervisor With Restart On Failure
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

    ${pid_before}=    Get Service PID    tedge-run-all

    ${operation}=    Create Service Command Operation
    ...    ${AGENT_XID}
    ...    {"command":"RESTART","serviceName":"tedge-agent","serviceType":"service"}
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

    # The whole process was re-executed, the agent having no unit of its own to restart.
    ${pid_after}=    Get Service PID    tedge-run-all
    Should Not Be Equal    ${pid_before}    ${pid_after}

    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y

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

Multiple mappers run under a single supervisor
    [Documentation]    The supervisor can host the c8y mapper and a custom flows-only mapper
    ...    simultaneously. Both report healthy and function independently: the c8y mapper
    ...    keeps talking to Cumulocity while the custom mapper processes its flow messages.
    [Setup]    Start Multi-Mapper Supervisor

    # All three components come up healthy inside the one process.
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-agent
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-test-echo

    # The c8y mapper keeps the device talking to Cumulocity.
    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-bridge-c8y
    External Identity Should Exist    ${DEVICE_SN}

    # The custom mapper processes flow messages independently of the c8y mapper.
    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub custom/test/in '{"value":42}'
    ${output}=    Should Have MQTT Messages
    ...    custom/test/out
    ...    minimum=1
    ...    date_from=${start}
    Should Contain    ${output[0]}    42

    [Teardown]    Stop Multi-Mapper Supervisor

SIGUSR1 restarts all mappers under a multi-mapper supervisor
    [Documentation]    SIGUSR1 restarts every mapper hosted by the supervisor. Both mappers
    ...    get a fresh health `time` while the process pid stays the same, proving each
    ...    mapper task was rebuilt independently without restarting the whole process.
    [Setup]    Start Multi-Mapper Supervisor

    # Baseline: capture health timestamps from both mappers.
    ${c8y_before}=    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-c8y
    ${echo_before}=    Wait Until Keyword Succeeds
    ...    60s    2s    Service Health Status Should Be Up    tedge-mapper-test-echo

    Execute Command    cmd=systemctl kill --signal=SIGUSR1 tedge-run-all.service

    # Both mappers were restarted (fresh health timestamps, same pid).
    Wait Until Keyword Succeeds
    ...    60s    2s    Mapper Restarted Since    ${c8y_before}
    Wait Until Keyword Succeeds
    ...    60s    2s    Custom Mapper Restarted Since    ${echo_before}

    # The agent was never targeted by SIGUSR1 and stayed up throughout.
    Service Health Status Should Be Up    tedge-agent

    [Teardown]    Stop Multi-Mapper Supervisor


*** Keywords ***
Register Device
    # Suite-level, so the (relatively expensive) Cumulocity device registration only
    # happens once. Self-signed registration keeps the device self-contained: the
    # certificate is generated and trusted locally, without depending on Cumulocity's
    # certificate-authority enrolment.
    ${DEVICE_SN}=    Setup    register_using=self-signed    connect=${False}
    Set Suite Variable    ${DEVICE_SN}
    Set Suite Variable    $AGENT_XID    ${DEVICE_SN}:device:main:service:tedge-agent
    Set Suite Variable    $MAPPER_XID    ${DEVICE_SN}:device:main:service:tedge-mapper-c8y

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

Create Custom Echo Mapper
    # Create a flows-only custom mapper that echoes messages from custom/test/in to
    # custom/test/out. No cloud credentials needed — it runs purely locally.
    Execute Command    mkdir -p /etc/tedge/mappers/test-echo/flows && chown -R tedge:tedge /etc/tedge/mappers/test-echo
    Execute Command
    ...    cmd=printf 'input.mqtt.topics = ["custom/test/in"]\nsteps = []\n\n[output.mqtt]\ntopic = "custom/test/out"\n' > /etc/tedge/mappers/test-echo/flows/echo.toml
    Execute Command    chown tedge:tedge /etc/tedge/mappers/test-echo/flows/echo.toml

Remove Custom Echo Mapper
    Execute Command    rm -rf /etc/tedge/mappers/test-echo

Start Multi-Mapper Supervisor
    Create Custom Echo Mapper
    Execute Command
    ...    cmd=systemd-run --unit=tedge-run-all --collect -p User=tedge -p Group=tedge /usr/bin/tedge run all c8y test-echo

Stop Multi-Mapper Supervisor
    Stop Supervisor
    Remove Custom Echo Mapper

Custom Mapper Restarted Since
    [Arguments]    ${before}
    ${now}=    Service Health Status Should Be Up    tedge-mapper-test-echo
    Should Be True    ${now["time"]} > ${before["time"]}
    Should Be Equal As Integers    ${now["pid"]}    ${before["pid"]}

Supported Service Commands Should Be
    [Arguments]    ${external_id}    @{expected}
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
    Wait Until Keyword Succeeds    30x    2s    Managed Object Service Commands Should Be    @{expected}

Managed Object Service Commands Should Be
    [Arguments]    @{expected}
    ${mo}=    Cumulocity.Managed Object Should Have Fragments    c8y_SupportedServiceCommands
    ${actual}=    Evaluate    sorted($mo["c8y_SupportedServiceCommands"])
    ${wanted}=    Evaluate    sorted($expected)
    Should Be Equal    ${actual}    ${wanted}

Create Service Command Operation
    [Arguments]    ${external_id}    ${fragment}
    Cumulocity.External Identity Should Exist    ${external_id}    show_info=${False}
    ${operation}=    Cumulocity.Create Operation
    ...    fragments={"c8y_ServiceCommand":${fragment}}
    ...    description=Service command
    RETURN    ${operation}

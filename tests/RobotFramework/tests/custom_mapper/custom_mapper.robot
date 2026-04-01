*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Suite Teardown

Test Tags           theme:custom_mapper    adapter:docker


*** Test Cases ***
Flows-only custom mapper starts and processes messages
    [Documentation]    A custom mapper with only a flows/ directory starts the flows engine and
    ...    processes messages without any cloud bridge. Health is reported on the expected topic.
    [Setup]    Create Flows Only Mapper    test-flows

    Start Service    tedge-mapper-test-flows
    ${pid}=    Service Should Be Running    tedge-mapper-test-flows

    # Health topic should appear for the mapper service
    ${health_start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-flows/cmd/health/check' ''
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-flows/status/health
    ...    minimum=1
    ...    date_from=${health_start}
    Should Contain    ${messages[0]}    "status":"up"

    # Flow processes messages: publish to custom/test/in, expect output on custom/test/out
    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub custom/test/in '{"value":42}'
    ${output}=    Should Have MQTT Messages
    ...    custom/test/out
    ...    minimum=1
    ...    date_from=${start}
    Should Contain    ${output[0]}    42

    [Teardown]    Stop And Remove Mapper    test-flows

Custom mapper with bridge forwards messages to remote broker
    [Documentation]    A custom mapper with bridge/ connects to a local test MQTT broker over TLS
    ...    and forwards messages from the local prefix to the remote prefix. Publishing to
    ...    custom-local/test on the local broker produces the message on custom-remote/test
    ...    on the remote broker.
    [Setup]    Create Bridge Mapper    test-bridge

    ${start}=    Get Unix Timestamp
    Start Service    tedge-mapper-test-bridge
    Service Should Be Running    tedge-mapper-test-bridge

    # Wait for the bridge to report it is connected to the remote broker
    ${bridge_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-bridge-test-bridge/status/health
    ...    minimum=1
    ...    date_from=${start}
    ...    message_contains="status":"up"

    # Subscribe on the remote broker in the background before publishing, so no messages are missed
    Execute Command
    ...    bash -c 'mosquitto_sub -p 18883 --cafile /tmp/tb-ca.crt -t "custom-remote/#" -C 1 -W 15 > /tmp/tb-received.txt 2>&1 &'
    Sleep    0.5s

    Execute Command    tedge mqtt pub custom-local/test '{"value":42}'

    # Poll until the bridged message arrives or the subscriber times out
    ${msg}=    Execute Command
    ...    timeout 12 bash -c 'until [ -s /tmp/tb-received.txt ]; do sleep 0.5; done; cat /tmp/tb-received.txt'
    Should Contain    ${msg}    42

    [Teardown]    Stop And Remove Mapper    test-bridge

Custom mapper with bridge using username/password auth forwards messages to remote broker
    [Documentation]    A custom mapper with bridge/ and credentials_path connects to a local test
    ...    MQTT broker over TLS using username/password authentication, and forwards messages from
    ...    the local prefix to the remote prefix.
    [Setup]    Create Password Bridge Mapper    test-password-bridge

    ${start}=    Get Unix Timestamp
    Start Service    tedge-mapper-test-password-bridge
    Service Should Be Running    tedge-mapper-test-password-bridge

    # Wait for the bridge to report it is connected to the remote broker
    ${bridge_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-bridge-test-password-bridge/status/health
    ...    minimum=1
    ...    date_from=${start}
    ...    message_contains="status":"up"

    # Subscribe on the remote (password) broker in the background before publishing
    Execute Command
    ...    bash -c 'mosquitto_sub -p 18884 --cafile /tmp/tb-ca.crt -u testuser -P testpass -t "custom-remote/#" -C 1 -W 15 > /tmp/tb-pw-received.txt 2>&1 &'
    Sleep    0.5s

    Execute Command    tedge mqtt pub custom-local/test '{"value":99}'

    # Poll until the bridged message arrives or the subscriber times out
    ${msg}=    Execute Command
    ...    timeout 12 bash -c 'until [ -s /tmp/tb-pw-received.txt ]; do sleep 0.5; done; cat /tmp/tb-pw-received.txt'
    Should Contain    ${msg}    99

    [Teardown]    Stop And Remove Mapper    test-password-bridge

Custom mapper with bridge, mapper.toml, and flows starts all subsystems
    [Documentation]    A custom mapper with all three components (mapper.toml, bridge/, flows/)
    ...    starts both the MQTT bridge and the flows engine. The mapper health topic appears and
    ...    the flows engine processes messages independently of the bridge connection state.
    [Setup]    Create Full Mapper    test-full

    Start Service    tedge-mapper-test-full
    ${pid}=    Service Should Be Running    tedge-mapper-test-full

    # Mapper health
    ${health_start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-full/cmd/health/check' ''
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-full/status/health
    ...    minimum=1
    ...    date_from=${health_start}
    Should Contain    ${messages[0]}    "pid":${pid}

    # Flows engine still processes messages even when bridge is configured
    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub custom/test/in '{"sensor":"temperature","value":25}'
    ${output}=    Should Have MQTT Messages
    ...    custom/test/out
    ...    minimum=1
    ...    date_from=${start}
    Should Contain    ${output[0]}    temperature

    [Teardown]    Stop And Remove Mapper    test-full

Two custom mapper profiles run concurrently without interfering
    [Documentation]    Two custom mapper profiles (alpha and beta) can run simultaneously as
    ...    independent services. Each has its own health topic and processes messages in isolation —
    ...    publishing to one mapper's input topic does not produce output on the other's output topic.
    [Setup]    Create Two Concurrent Mappers    test-alpha    test-beta

    Start Service    tedge-mapper-test-alpha
    Start Service    tedge-mapper-test-beta
    Service Should Be Running    tedge-mapper-test-alpha
    Service Should Be Running    tedge-mapper-test-beta

    # Both mappers report health independently
    ${alpha_health_start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-alpha/cmd/health/check' ''
    ${alpha_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-alpha/status/health
    ...    minimum=1
    ...    date_from=${alpha_health_start}
    Should Contain    ${alpha_health[0]}    "status":"up"

    ${beta_health_start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-beta/cmd/health/check' ''
    ${beta_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-beta/status/health
    ...    minimum=1
    ...    date_from=${beta_health_start}
    Should Contain    ${beta_health[0]}    "status":"up"

    # Messages to alpha do not appear on beta's output and vice versa
    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub alpha/test/in '{"source":"alpha"}'
    Should Have MQTT Messages    alpha/test/out    minimum=1    date_from=${start}    message_contains=alpha

    Should Not Have MQTT Messages    beta/test/out    date_from=${start}

    ${start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub beta/test/in '{"source":"beta"}'
    Should Have MQTT Messages    beta/test/out    minimum=1    date_from=${start}    message_contains=beta

    Should Not Have MQTT Messages    alpha/test/out    date_from=${start}

    [Teardown]    Stop And Remove Two Mappers    test-alpha    test-beta

Custom mapper with bridge.tls.enable false forwards messages over plain TCP
    [Documentation]    A custom mapper with `bridge.tls.enable = "false"` connects to a plain (non-TLS) MQTT
    ...    broker and forwards messages. This proves the TLS-skip code path works end-to-end.
    [Setup]    Create Non TLS Bridge Mapper    test-notls

    ${start}=    Get Unix Timestamp
    Start Service    tedge-mapper-test-notls
    Service Should Be Running    tedge-mapper-test-notls

    # Wait for the bridge to report healthy
    ${bridge_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-bridge-test-notls/status/health
    ...    minimum=1
    ...    date_from=${start}
    ...    message_contains="status":"up"

    # Subscribe on the plain-TCP remote broker before publishing (credentials required)
    Execute Command
    ...    bash -c 'mosquitto_sub -p 18885 -u testuser -P testpass -t "custom-remote/#" -C 1 -W 15 > /tmp/tb-notls-received.txt 2>&1 &'
    Sleep    0.5s

    Execute Command    tedge mqtt pub custom-local/test '{"value":77}'

    ${msg}=    Execute Command
    ...    timeout 12 bash -c 'until [ -s /tmp/tb-notls-received.txt ]; do sleep 0.5; done; cat /tmp/tb-notls-received.txt'
    Should Contain    ${msg}    77

    [Teardown]    Stop And Remove Mapper    test-notls

Stale bridge health is cleared when mapper switches to flows-only
    [Documentation]    When a mapper previously ran with a bridge (leaving a retained "up" on the
    ...    bridge health topic), and is then reconfigured to flows-only and restarted, the mapper
    ...    publishes an empty retained message to the bridge health topic. This deregisters the
    ...    stale bridge service from the entity store.
    [Setup]    Create Bridge Mapper    test-stale

    # Phase 1: Start the bridge mapper so it leaves a retained "up" on the bridge health topic
    ${phase1_start}=    Get Unix Timestamp
    Start Service    tedge-mapper-test-stale
    Service Should Be Running    tedge-mapper-test-stale

    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-bridge-test-stale/status/health
    ...    minimum=1
    ...    date_from=${phase1_start}
    ...    message_contains="status":"up"

    # Stop the mapper; the retained "up" message is still on the broker
    Execute Command    systemctl stop tedge-mapper-test-stale

    # Phase 2: Reconfigure to flows-only (remove bridge/, add flows/)
    Execute Command    rm -rf /etc/tedge/mappers/test-stale/bridge
    Execute Command
    ...    mkdir -p /etc/tedge/mappers/test-stale/flows && chown -R tedge:tedge /etc/tedge/mappers/test-stale
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/echo.toml    /etc/tedge/mappers/test-stale/flows/

    # Phase 3: Restart in flows-only mode — should clear the stale bridge health
    Start Service    tedge-mapper-test-stale
    Service Should Be Running    tedge-mapper-test-stale

    # The stale retained "up" should now be replaced by an empty message.
    # Verify by checking that `tedge mapper test` does NOT report the bridge as healthy
    # (the mapper health check should pass, but no bridge check should occur since
    # there is no bridge/ directory).

    # Wait for mapper to report healthy first
    ${health_start}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-stale/cmd/health/check' ''
    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-stale/status/health
    ...    minimum=1
    ...    date_from=${health_start}
    ...    message_contains="status":"up"

    # Verify the bridge health topic no longer has a non-empty retained message.
    # Subscribe with -C 1 -W 3 — if only an empty retained message exists, mosquitto_sub
    # returns nothing (empty retained payloads clear the retained state).
    ${bridge_msg}=    Execute Command
    ...    timeout 5 mosquitto_sub -t 'te/device/main/service/tedge-mapper-bridge-test-stale/status/health' -C 1 -W 3 || true
    ...    strip=${True}
    Should Be Empty    ${bridge_msg}

    [Teardown]    Stop And Remove Mapper    test-stale

tedge connect --test succeeds for a running custom bridge mapper
    [Documentation]    When a bridge custom mapper is running, `tedge connect <name> --test`
    ...    should wait for both the mapper and bridge health topics to report "up".
    [Setup]    Create Bridge Mapper    test-connect-bridge

    Start Service    tedge-mapper-test-connect-bridge
    Service Should Be Running    tedge-mapper-test-connect-bridge

    ${output}=    Execute Command
    ...    sudo tedge connect test-connect-bridge --test
    ...    stdout=${False}    stderr=${True}
    Should Contain    ${output}    tedge-mapper-test-connect-bridge
    Should Contain    ${output}    tedge-mapper-bridge-test-connect-bridge

    [Teardown]    Stop And Remove Mapper    test-connect-bridge

tedge connect --test succeeds for a running flows-only custom mapper
    [Documentation]    When a flows-only custom mapper is running, `tedge connect <name> --test`
    ...    should wait only for the mapper health topic (no bridge phase) and succeed.
    [Setup]    Create Flows Only Mapper    test-connect-flows

    Start Service    tedge-mapper-test-connect-flows
    Service Should Be Running    tedge-mapper-test-connect-flows

    ${output}=    Execute Command
    ...    sudo tedge connect test-connect-flows --test
    ...    stdout=${False}    stderr=${True}
    Should Contain    ${output}    tedge-mapper-test-connect-flows
    Should Not Contain    ${output}    tedge-mapper-bridge-test-connect-flows

    [Teardown]    Stop And Remove Mapper    test-connect-flows

tedge connect --test fails when the custom mapper is not running
    [Documentation]    When the mapper service is not started, `tedge connect <name> --test`
    ...    should time out waiting for health and exit with a non-zero code.
    [Setup]    Create Flows Only Mapper    test-connect-down

    ${output}=    Execute Command
    ...    sudo tedge connect test-connect-down --test
    ...    exp_exit_code=1
    ...    stdout=${False}    stderr=${True}
    Should Contain    ${output}    tedge-mapper-test-connect-down

    [Teardown]    Stop And Remove Mapper    test-connect-down

tedge connect without --test gives a helpful error for custom mappers
    [Documentation]    Running `tedge connect <custom-mapper>` without --test is not supported
    ...    and should exit non-zero with a message that mentions --test.
    [Setup]    Create Flows Only Mapper    test-connect-no-test

    ${output}=    Execute Command
    ...    sudo tedge connect test-connect-no-test
    ...    exp_exit_code=1
    ...    stdout=${False}    stderr=${True}
    Should Contain    ${output}    --test

    [Teardown]    Stop And Remove Mapper    test-connect-no-test


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Start Remote TLS Broker

Suite Teardown
    Get Suite Logs
    Stop Remote TLS Broker

Start Remote TLS Broker
    # CA cert — rustls rejects a self-signed end-entity cert used directly as a trust anchor
    Execute Command
    ...    cmd=openssl req -x509 -newkey rsa:2048 -keyout /tmp/tb-ca.key -out /tmp/tb-ca.crt -days 1 -nodes -subj "/CN=test-ca" 2>/dev/null
    # Server cert signed by the CA, with SANs so rustls hostname validation passes
    Execute Command
    ...    cmd=openssl req -newkey rsa:2048 -keyout /tmp/tb-server.key -out /tmp/tb-server.csr -nodes -subj "/CN=localhost" 2>/dev/null
    Execute Command
    ...    cmd=printf "subjectAltName=DNS:localhost,IP:127.0.0.1" > /tmp/tb-server.ext && openssl x509 -req -in /tmp/tb-server.csr -CA /tmp/tb-ca.crt -CAkey /tmp/tb-ca.key -CAcreateserial -out /tmp/tb-server.crt -days 1 -extfile /tmp/tb-server.ext 2>/dev/null
    Execute Command    chmod 644 /tmp/tb-server.key
    # Client cert for the mapper — the test broker does not require or verify client certs
    Execute Command
    ...    cmd=openssl req -x509 -newkey rsa:2048 -keyout /tmp/tb-client.key -out /tmp/tb-client.crt -days 1 -nodes -subj "/CN=test-mapper" 2>/dev/null
    Execute Command    chmod 644 /tmp/tb-client.key
    # Write mosquitto config: server-only TLS, no client cert required, anonymous allowed
    Execute Command
    ...    sh -c 'echo listener 18883 > /tmp/tb.conf; echo certfile /tmp/tb-server.crt >> /tmp/tb.conf; echo keyfile /tmp/tb-server.key >> /tmp/tb.conf; echo allow_anonymous true >> /tmp/tb.conf; echo log_dest file /tmp/tb-mosquitto.log >> /tmp/tb.conf'
    Execute Command    mosquitto -c /tmp/tb.conf -d
    Sleep    0.5s
    # Password broker on port 18884 — same TLS certs, username/password required, no anonymous
    Execute Command    mosquitto_passwd -c -b /tmp/tb-passwd.txt testuser testpass
    Execute Command
    ...    sh -c 'echo listener 18884 > /tmp/tb-passwd.conf; echo certfile /tmp/tb-server.crt >> /tmp/tb-passwd.conf; echo keyfile /tmp/tb-server.key >> /tmp/tb-passwd.conf; echo allow_anonymous false >> /tmp/tb-passwd.conf; echo password_file /tmp/tb-passwd.txt >> /tmp/tb-passwd.conf; echo log_dest file /tmp/tb-passwd-mosquitto.log >> /tmp/tb-passwd.conf'
    Execute Command    mosquitto -c /tmp/tb-passwd.conf -d
    Sleep    0.5s
    # Plain TCP broker on port 18885 — no TLS, password auth, for bridge.tls=off testing
    Execute Command
    ...    sh -c 'echo listener 18885 > /tmp/tb-notls.conf; echo allow_anonymous false >> /tmp/tb-notls.conf; echo password_file /tmp/tb-passwd.txt >> /tmp/tb-notls.conf; echo log_dest file /tmp/tb-notls-mosquitto.log >> /tmp/tb-notls.conf'
    Execute Command    mosquitto -c /tmp/tb-notls.conf -d
    Sleep    0.5s

Stop Remote TLS Broker
    Execute Command    cat /tmp/tb-mosquitto.log    ignore_exit_code=True
    Execute Command    cat /tmp/tb-passwd-mosquitto.log    ignore_exit_code=True
    Execute Command    cat /tmp/tb-notls-mosquitto.log    ignore_exit_code=True
    Execute Command    pkill -f 'mosquitto -c /tmp/tb.conf'    ignore_exit_code=True
    Execute Command    pkill -f 'mosquitto -c /tmp/tb-passwd.conf'    ignore_exit_code=True
    Execute Command    pkill -f 'mosquitto -c /tmp/tb-notls.conf'    ignore_exit_code=True
    Execute Command
    ...    rm -f /tmp/tb-ca.crt /tmp/tb-ca.key /tmp/tb-ca.srl /tmp/tb-server.crt /tmp/tb-server.key /tmp/tb-server.csr /tmp/tb-server.ext /tmp/tb-client.crt /tmp/tb-client.key /tmp/tb.conf /tmp/tb-mosquitto.log /tmp/tb-received.txt /tmp/tb-passwd.txt /tmp/tb-passwd.conf /tmp/tb-passwd-mosquitto.log /tmp/tb-pw-received.txt /tmp/tb-notls.conf /tmp/tb-notls-mosquitto.log /tmp/tb-notls-received.txt
    ...    timeout=0
    ...    retries=0

Create Mapper Service File
    [Arguments]    ${name}
    Execute Command
    ...    cmd=sh -c 'printf "[Unit]\nDescription=thin-edge.io user-defined mapper ${name}\nAfter=syslog.target network.target mosquitto.service\n\n[Service]\nUser=tedge\nExecStartPre=+-/usr/bin/tedge init\nExecStart=/usr/bin/tedge-mapper ${name}\nRestart=on-failure\nRestartPreventExitStatus=255\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n" > /etc/systemd/system/tedge-mapper-${name}.service'
    Execute Command    systemctl daemon-reload

Create Flows Only Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/flows && chown -R tedge:tedge /etc/tedge/mappers/${name}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/echo.toml    /etc/tedge/mappers/${name}/flows/
    Create Mapper Service File    ${name}

Create Bridge Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/bridge && chown -R tedge:tedge /etc/tedge/mappers/${name}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mapper.toml    /etc/tedge/mappers/${name}/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/bridge/rules.toml    /etc/tedge/mappers/${name}/bridge/
    Create Mapper Service File    ${name}

Create Password Bridge Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/bridge && chown -R tedge:tedge /etc/tedge/mappers/${name}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mapper-password.toml    /etc/tedge/mappers/${name}/mapper.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/credentials.toml    /etc/tedge/mappers/${name}/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/bridge/rules.toml    /etc/tedge/mappers/${name}/bridge/
    Create Mapper Service File    ${name}

Create Full Mapper
    [Arguments]    ${name}
    Execute Command
    ...    mkdir -p /etc/tedge/mappers/${name}/bridge /etc/tedge/mappers/${name}/flows && chown -R tedge:tedge /etc/tedge/mappers/${name}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mapper.toml    /etc/tedge/mappers/${name}/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/bridge/rules.toml    /etc/tedge/mappers/${name}/bridge/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/echo.toml    /etc/tedge/mappers/${name}/flows/
    Create Mapper Service File    ${name}

Create Non TLS Bridge Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/bridge && chown -R tedge:tedge /etc/tedge/mappers/${name}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mapper-notls.toml    /etc/tedge/mappers/${name}/mapper.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/credentials.toml    /etc/tedge/mappers/${name}/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/bridge/rules.toml    /etc/tedge/mappers/${name}/bridge/
    Create Mapper Service File    ${name}

Create Two Concurrent Mappers
    [Arguments]    ${name_a}    ${name_b}
    Execute Command    mkdir -p /etc/tedge/mappers/${name_a}/flows && chown -R tedge:tedge /etc/tedge/mappers/${name_a}
    Execute Command    mkdir -p /etc/tedge/mappers/${name_b}/flows && chown -R tedge:tedge /etc/tedge/mappers/${name_b}
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/alpha-echo.toml    /etc/tedge/mappers/${name_a}/flows/echo.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/beta-echo.toml    /etc/tedge/mappers/${name_b}/flows/echo.toml
    Create Mapper Service File    ${name_a}
    Create Mapper Service File    ${name_b}

Collect Mapper Logs
    [Arguments]    ${name}
    Execute Command    systemctl status tedge-mapper-${name} --no-pager || true
    Execute Command    journalctl -u tedge-mapper-${name} --no-pager -n 100 || true

Stop And Remove Mapper
    [Arguments]    ${name}
    Run Keyword And Ignore Error    Collect Mapper Logs    ${name}
    Execute Command    systemctl stop tedge-mapper-${name} || true
    Execute Command    rm -f /etc/systemd/system/tedge-mapper-${name}.service
    Execute Command    systemctl daemon-reload
    Execute Command    rm -rf /etc/tedge/mappers/${name}

Stop And Remove Two Mappers
    [Arguments]    ${name_a}    ${name_b}
    Run Keyword And Ignore Error    Collect Mapper Logs    ${name_a}
    Run Keyword And Ignore Error    Collect Mapper Logs    ${name_b}
    Execute Command    systemctl stop tedge-mapper-${name_a} || true
    Execute Command    systemctl stop tedge-mapper-${name_b} || true
    Execute Command    rm -f /etc/systemd/system/tedge-mapper-${name_a}.service
    Execute Command    rm -f /etc/systemd/system/tedge-mapper-${name_b}.service
    Execute Command    systemctl daemon-reload
    Execute Command    rm -rf /etc/tedge/mappers/${name_a}
    Execute Command    rm -rf /etc/tedge/mappers/${name_b}

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
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-flows/cmd/health/check' ''
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-flows/status/health
    ...    minimum=1
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

    Start Service    tedge-mapper-test-bridge
    Service Should Be Running    tedge-mapper-test-bridge

    # Wait for the bridge to report it is connected to the remote broker
    ${bridge_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-bridge-test-bridge/status/health
    ...    minimum=1
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

    Start Service    tedge-mapper-test-password-bridge
    Service Should Be Running    tedge-mapper-test-password-bridge

    # Wait for the bridge to report it is connected to the remote broker
    ${bridge_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-bridge-test-password-bridge/status/health
    ...    minimum=1
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
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-full/cmd/health/check' ''
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-full/status/health
    ...    minimum=1
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
    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-alpha/cmd/health/check' ''
    ${alpha_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-alpha/status/health
    ...    minimum=1
    Should Contain    ${alpha_health[0]}    "status":"up"

    Execute Command    tedge mqtt pub 'te/device/main/service/tedge-mapper-test-beta/cmd/health/check' ''
    ${beta_health}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-mapper-test-beta/status/health
    ...    minimum=1
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

Stop Remote TLS Broker
    Execute Command    cat /tmp/tb-mosquitto.log    ignore_exit_code=True
    Execute Command    cat /tmp/tb-passwd-mosquitto.log    ignore_exit_code=True
    Execute Command    pkill -f 'mosquitto -c /tmp/tb.conf'    ignore_exit_code=True
    Execute Command    pkill -f 'mosquitto -c /tmp/tb-passwd.conf'    ignore_exit_code=True
    Execute Command
    ...    rm -f /tmp/tb-ca.crt /tmp/tb-ca.key /tmp/tb-ca.srl /tmp/tb-server.crt /tmp/tb-server.key /tmp/tb-server.csr /tmp/tb-server.ext /tmp/tb-client.crt /tmp/tb-client.key /tmp/tb.conf /tmp/tb-mosquitto.log /tmp/tb-received.txt /tmp/tb-passwd.txt /tmp/tb-passwd.conf /tmp/tb-passwd-mosquitto.log /tmp/tb-pw-received.txt
    ...    timeout=0
    ...    retries=0

Create Mapper Service File
    [Arguments]    ${name}
    Execute Command
    ...    cmd=sh -c 'printf "[Unit]\nDescription=thin-edge.io user-defined mapper ${name}\nAfter=syslog.target network.target mosquitto.service\n\n[Service]\nUser=tedge\nExecStartPre=+-/usr/bin/tedge init\nExecStart=/usr/bin/tedge-mapper ${name}\nRestart=on-failure\nRestartPreventExitStatus=255\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n" > /etc/systemd/system/tedge-mapper-${name}.service'
    Execute Command    systemctl daemon-reload

Create Flows Only Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/flows
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/echo.toml    /etc/tedge/mappers/${name}/flows/
    Create Mapper Service File    ${name}

Create Bridge Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/bridge
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mapper.toml    /etc/tedge/mappers/${name}/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/bridge/rules.toml    /etc/tedge/mappers/${name}/bridge/
    Create Mapper Service File    ${name}

Create Password Bridge Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/bridge
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mapper-password.toml    /etc/tedge/mappers/${name}/mapper.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/credentials.toml    /etc/tedge/mappers/${name}/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/bridge/rules.toml    /etc/tedge/mappers/${name}/bridge/
    Create Mapper Service File    ${name}

Create Full Mapper
    [Arguments]    ${name}
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/bridge
    Execute Command    mkdir -p /etc/tedge/mappers/${name}/flows
    ThinEdgeIO.Transfer To Device    ${CURDIR}/mapper.toml    /etc/tedge/mappers/${name}/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/bridge/rules.toml    /etc/tedge/mappers/${name}/bridge/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/flows/echo.toml    /etc/tedge/mappers/${name}/flows/
    Create Mapper Service File    ${name}

Create Two Concurrent Mappers
    [Arguments]    ${name_a}    ${name_b}
    Execute Command    mkdir -p /etc/tedge/mappers/${name_a}/flows
    Execute Command    mkdir -p /etc/tedge/mappers/${name_b}/flows
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

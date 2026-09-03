*** Settings ***
Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:bootstrap    theme:plugins


*** Variables ***
${EXAMPLES}     ${CURDIR}/examples
${HOOKS}        /usr/share/tedge/bootstrap.d


*** Test Cases ***
Preflight Passes For A Reachable Server
    [Documentation]    A run with dummy bootstrap credentials stops cheaply at
    ...    the register step - proving the pipeline made it through the
    ...    prepare phase, i.e. the preflight passed
    Install Example Files    preflight/prepare.d/05_preflight    prepare.d
    ${domain}=    Cumulocity.Get Domain
    ${output}=    Execute Command
    ...    cmd=env C8Y_BOOTSTRAP_PASSWORD=dummy tedge bootstrap c8y --url "${domain}" --device-id "${DEVICE_SN}" --register basic 2>&1
    ...    exp_exit_code=!0
    Should Not Contain    ${output}    Preflight failed
    Should Contain    ${output}    rejected

Preflight Blocks An Unreachable Server Before Anything Is Written
    Install Example Files    preflight/prepare.d/05_preflight    prepare.d
    ${output}=    Execute Command
    ...    cmd=env C8Y_BOOTSTRAP_PASSWORD=dummy tedge bootstrap c8y --url tedge-does-not-exist.invalid --device-id "${DEVICE_SN}" --register basic 2>&1
    ...    exp_exit_code=!0
    Should Contain    ${output}    cannot resolve
    # aborted in the prepare phase: nothing was configured
    Execute Command    cmd=tedge config get c8y.url    exp_exit_code=!0

Preflight Skips Itself On An Offline Run
    Install Example Files    preflight/prepare.d/05_preflight    prepare.d
    ${domain}=    Cumulocity.Get Domain
    ${output}=    Execute Command
    ...    cmd=tedge bootstrap c8y --url "${domain}" --offline --verbose 2>&1
    Should Contain    ${output}    skipping the preflight

Trust Server Certificate On First Use
    [Documentation]    The run fails later (the local TLS fixture is not a
    ...    Cumulocity endpoint) - the assertions cover the prepare hook:
    ...    the presented certificate lands in the system CA store,
    ...    and a re-run finds the endpoint already trusted (exit 2)
    Install Example Files    trust-server-cert/prepare.d/10_trust_server_cert    prepare.d
    Start Self Signed TLS Server    port=8443
    ${output}=    Execute Command
    ...    cmd=timeout 90 env C8Y_BOOTSTRAP_PASSWORD=dummy tedge bootstrap c8y --url localhost:8443 --device-id "${DEVICE_SN}" --register basic 2>&1
    ...    exp_exit_code=!0
    Should Contain    ${output}    trust-on-first-use
    Execute Command    cmd=test -f /usr/local/share/ca-certificates/tedge-bootstrap-localhost.crt
    ${output}=    Execute Command
    ...    cmd=timeout 90 env C8Y_BOOTSTRAP_PASSWORD=dummy tedge bootstrap c8y --url localhost:8443 --device-id "${DEVICE_SN}" --register basic --verbose 2>&1
    ...    exp_exit_code=!0
    Should Contain    ${output}    already trusted

Nearest Endpoint Is Selected Without A URL
    Install Example Files    nearest-endpoint/prepare.d/20_nearest_endpoint    prepare.d
    ${domain}=    Cumulocity.Get Domain
    Execute Command
    ...    cmd=env BOOTSTRAP_ENDPOINT_CANDIDATES="tedge-dead-endpoint.invalid ${domain}" C8Y_BOOTSTRAP_PASSWORD=dummy tedge bootstrap c8y --device-id "${DEVICE_SN}" --register basic
    ...    exp_exit_code=!0
    ${url}=    Execute Command    cmd=tedge config get c8y.url    strip=True
    Should Be Equal    ${url}    ${domain}

Device Id Is Generated From The Hardware Identity
    Install Example Files    generated-device-id/prepare.d/15_device_id    prepare.d
    ${domain}=    Cumulocity.Get Domain
    Execute Command    cmd=tedge bootstrap c8y --url "${domain}" --offline
    ${device_id}=    Execute Command    cmd=tedge config get device.id    strip=True
    Should Match Regexp    ${device_id}    ^tedge-.+

Proxy Question Is Added By A Site Descriptor Override
    Install Example Files    proxy-settings/clouds.d/c8y.toml    clouds.d
    ${output}=    Execute Command    cmd=tedge bootstrap c8y --describe
    Should Contain    ${output}    proxy.address (global)
    # the override restates the basic method: the compiled-in inputs are kept
    Should Contain    ${output}    $C8Y_BOOTSTRAP_PASSWORD

Hidden Clouds Are Not Offered But Still Work
    Install Example Files    hide-clouds/clouds.d/az.ignore    clouds.d
    Install Example Files    hide-clouds/clouds.d/aws.ignore    clouds.d
    ${output}=    Execute Command    cmd=tedge bootstrap --describe
    Should Contain    ${output}    Cumulocity (c8y)
    Should Not Contain    ${output}    Azure IoT Hub
    Should Not Contain    ${output}    AWS IoT Core
    # hiding curates the pick-list, it does not disable the cloud
    ${output}=    Execute Command    cmd=tedge bootstrap az --describe
    Should Contain    ${output}    not offered by the wizard

Captured Invocations Are Checked Upfront Before Replaying
    # not /tmp: it is a tmpfs mount in the systemd container,
    # which docker file transfers cannot reach
    Transfer To Device    ${EXAMPLES}/invocations/two-instances.json    /etc/tedge/
    ${output}=    Execute Command
    ...    cmd=tedge bootstrap --from /etc/tedge/two-instances.json --dry-run 2>&1
    ...    exp_exit_code=!0
    Should Contain    ${output}    C8Y_BOOTSTRAP_PASSWORD
    Should Contain    ${output}    ACME_TOKEN
    Execute Command
    ...    cmd=env C8Y_BOOTSTRAP_PASSWORD=dummy ACME_TOKEN=dummy tedge bootstrap --from /etc/tedge/two-instances.json --dry-run

A Custom Cloud Registers Via Its Packaged Hook
    # Transfer To Device    ${CURDIR}/examples/custom-cloud/tedge-mapper-acme.service    dst=/lib/systemd/system/
    Install Example Files    custom-cloud/clouds.d/acme.toml    clouds.d
    Install Example Files    custom-cloud/register.d/40_acme    register.d
    # method validation speaks the descriptor's vocabulary
    ${output}=    Execute Command
    ...    cmd=tedge bootstrap acme --url acme.example.com --register nope 2>&1
    ...    exp_exit_code=!0
    Should Contain    ${output}    available: token
    # the register hook fulfils registration
    # (the connect outcome depends on whether a tedge-mapper-acme
    # service can start on the device, so it is not asserted here)
    Execute Command
    ...    cmd=tedge bootstrap acme --url acme.example.com --device-id acme01 --register token
    ...    ignore_exit_code=${True}
    ${credentials}=    Execute Command    cmd=cat /etc/tedge/mappers/acme/credentials.toml
    Should Contain    ${credentials}    test-token

Offline Firstboot Composition Saves A Replayable Invocation
    ${domain}=    Cumulocity.Get Domain
    Execute Command
    ...    cmd=tedge bootstrap c8y --url "${domain}" --device-id firstboot01 --register basic --offline --save /etc/tedge/bootstrap.json
    # the saved invocation lists the required inputs by name (never values)
    ${saved}=    Execute Command    cmd=cat /etc/tedge/bootstrap.json
    Should Contain    ${saved}    C8Y_BOOTSTRAP_PASSWORD
    Should Not Contain    ${saved}    offline
    # the completion replay refuses to start while the environment is incomplete
    ${output}=    Execute Command
    ...    cmd=tedge bootstrap --from /etc/tedge/bootstrap.json 2>&1
    ...    exp_exit_code=!0
    Should Contain    ${output}    C8Y_BOOTSTRAP_PASSWORD

Mosquitto Is Exposed To External Services
    Install Example Files
    ...    mosquitto-auto-config/configure.d/90_expose_to_external_services    configure.d
    ${domain}=    Cumulocity.Get Domain
    Execute Command    cmd=tedge config set mqtt.bind.address 0.0.0.0
    Execute Command    cmd=tedge bootstrap c8y --url "${domain}" --offline

    # bind to all adapters
    ${mqtt_bind_address}=    Execute Command    cmd=tedge config get mqtt.bind.address    strip=${True}
    ${http_bind_address}=    Execute Command    cmd=tedge config get http.bind.address    strip=${True}
    ${c8y_bind_address}=    Execute Command    cmd=tedge config get c8y.proxy.bind.address    strip=${True}
    Should Be Equal    ${mqtt_bind_address}    0.0.0.0
    Should Be Equal    ${http_bind_address}    0.0.0.0
    Should Be Equal    ${c8y_bind_address}    0.0.0.0

    # publish host value
    ${hostname}=    Execute Command    cmd=hostname    strip=True
    ${mqtt_client_host}=    Execute Command    cmd=tedge config get mqtt.client.host    strip=${True}
    ${http_client_host}=    Execute Command    cmd=tedge config get http.client.host    strip=${True}
    ${c8y_client_host}=    Execute Command    cmd=tedge config get c8y.proxy.client.host    strip=${True}
    Should Be Equal    ${mqtt_client_host}    ${hostname}
    Should Be Equal    ${http_client_host}    ${hostname}
    Should Be Equal    ${c8y_client_host}    ${hostname}

An Available HSM Is Detected And Configured
    Install Example Files    detect-hsm/configure.d/10_configure-hsm    configure.d
    ${domain}=    Cumulocity.Get Domain
    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}
    Execute Command
    ...    cmd=sudo env DEVICE_ONE_TIME_PASSWORD='${credentials.one_time_password}' tedge bootstrap c8y --url '${domain}' --device-id '${DEVICE_SN}'
    ${cryptoki_mode}=    Execute Command    cmd=tedge config get device.cryptoki.mode    strip=${True}
    Should Be Equal    ${cryptoki_mode}    socket


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    $DEVICE_SN
    Execute Command
    ...    cmd=mkdir -p ${HOOKS}/prepare.d ${HOOKS}/configure.d ${HOOKS}/register.d ${HOOKS}/finalize.d ${HOOKS}/clouds.d

Install Example Files
    [Documentation]    Install one file of an example into a bootstrap.d
    ...    drop-in directory, keeping hooks executable
    [Arguments]    ${example_path}    ${target_dir}
    Transfer To Device    ${EXAMPLES}/${example_path}    ${HOOKS}/${target_dir}/
    IF    "${target_dir}" != "clouds.d"
        Execute Command    cmd=chmod +x ${HOOKS}/${target_dir}/*
    END

Start Self Signed TLS Server
    [Documentation]    A local TLS endpoint presenting a certificate the
    ...    system does not trust (the trust-on-first-use fixture)
    [Arguments]    ${port}=8443
    Execute Command
    ...    cmd=openssl req -x509 -newkey rsa:2048 -keyout /tmp/tls-test.key -out /tmp/tls-test.crt -days 1 -nodes -subj "/CN=localhost"
    Execute Command
    ...    cmd=systemd-run --unit=tls-test openssl s_server -accept ${port} -cert /tmp/tls-test.crt -key /tmp/tls-test.key -www
    # wait for the listener to come up
    Execute Command
    ...    cmd=timeout 10 sh -c 'until openssl s_client -connect localhost:${port} </dev/null >/dev/null 2>&1; do sleep 0.5; done'

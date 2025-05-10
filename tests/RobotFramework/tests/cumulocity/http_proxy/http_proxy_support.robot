*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Teardown       Get Logs

Test Tags           theme:c8y    theme:http_proxy


*** Test Cases ***
tedge can connect to Cumulocity using HTTP Connect Tunnelling
    [Setup]    Setup Device With thin-edge.io
    # Check that proxy is working
    Execute Command
    ...    cmd=env http_proxy=127.0.0.1:8080 HTTP_PROXY=127.0.0.1:8080 https_proxy=127.0.0.1:8080 HTTPS_PROXY=127.0.0.1:8080 curl -f --max-time 5 https://google.com
    ...    retries=0
    Execute Command    cmd=curl -f --max-time 5 https://google.com    exp_exit_code=!0
    Execute Command    tedge connect c8y
    Device Should Exist    ${DEVICE_SN}
    ${operation}=    Get Configuration    tedge-configuration-plugin
    Operation Should Be SUCCESSFUL    ${operation}

Remote Access Uses the HTTP Proxy
    [Tags]    theme:c8y    theme:http_proxy    theme:remoteaccess
    [Setup]    Setup Device With thin-edge.io
    Execute Command    tedge connect c8y
    Device Should Exist    ${DEVICE_SN}
    Execute SSH Command Using Remote Access

C8Y HTTP proxy uses HTTP tunnelling
    [Setup]    Setup Device With thin-edge.io
    Execute Command    tedge connect c8y
    Execute Command    tedge http get /c8y/inventory/managedObjects

Install thin-edge.io behind a Proxy using wget
    [Setup]    Setup Device Without thin-edge.io
    Execute Command
    ...    cmd=export https_proxy=http://127.0.0.1:8080; wget -O - https://thin-edge.io/install.sh | sh -s
    Configure and Connect to Cumulocity

Install thin-edge.io behind a Proxy using curl
    [Setup]    Setup Device Without thin-edge.io
    Execute Command
    ...    cmd=export https_proxy=http://127.0.0.1:8080; curl -fsSL https://thin-edge.io/install.sh | sh -s
    Configure and Connect to Cumulocity


*** Keywords ***
Setup Device With thin-edge.io
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true

    Set Suite Variable    $DEVICE_SN
    Add iptables Rules
    Configure tedge to use HTTP Proxy
    Start Service    gost-http-proxy

Setup Device Without thin-edge.io
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $DEVICE_SN
    Add iptables Rules
    Start Service    gost-http-proxy

Configure and Connect to Cumulocity
    ${domain}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${domain}"
    Execute Command    tedge cert create --device-id "${DEVICE_SN}"
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y
    Execute Command    tedge connect c8y

Configure tedge to use HTTP Proxy
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set proxy.address http://127.0.0.1:8080

Add iptables Rules
    # Set default outbound policy to DROP
    Execute Command    sudo iptables -P OUTPUT DROP
    # Allow loopback traffic
    Execute Command    sudo iptables -A OUTPUT -o lo -j ACCEPT
    # Allow traffic from the gost user
    Execute Command    sudo iptables -A OUTPUT -m owner --uid-owner "$(id -u gost)" -j ACCEPT

Execute SSH Command Using Remote Access
    ${KEY_FILE}=    ThinEdgeIO.Configure SSH
    ThinEdgeIO.Add Remote Access Passthrough Configuration
    ${stdout}=    ThinEdgeIO.Execute Remote Access Command
    ...    command=tedge --version
    ...    exp_exit_code=0
    ...    user=root
    ...    key_file=${KEY_FILE}
    Should Match Regexp    ${stdout}    tedge .+

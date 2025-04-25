*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:http_proxy


*** Test Cases ***
tedge can connect to Cumulocity using HTTP Connect Tunnelling
    # Check that proxy is working
    Execute Command
    ...    cmd=env http_proxy=127.0.0.1:8080 HTTP_PROXY=127.0.0.1:8080 https_proxy=127.0.0.1:8080 HTTPS_PROXY=127.0.0.1:8080 curl -f --max-time 5 https://google.com
    ...    retries=0
    Execute Command    cmd=curl -f --max-time 5 https://google.com    exp_exit_code=!0
    Execute Command    tedge connect c8y
    Device Should Exist    ${DEVICE_SN}
    ${operation}=    Get Configuration    tedge-configuration-plugin
    Operation Should Be SUCCESSFUL    ${operation}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true

    Set Suite Variable    $DEVICE_SN
    Add iptables Rules
    Configure tedge to use HTTP Proxy
    Start Service    gost-http-proxy

Configure tedge to use HTTP Proxy
    Execute Command    tedge config set mqtt.bridge.built_in true
    Execute Command    tedge config set proxy.type http
    Execute Command    tedge config set proxy.address 127.0.0.1:8080

Add iptables Rules
    # Set default outbound policy to DROP
    Execute Command    sudo iptables -P OUTPUT DROP
    # Allow loopback traffic
    Execute Command    sudo iptables -A OUTPUT -o lo -j ACCEPT
    # Allow traffic from the gost user
    Execute Command    sudo iptables -A OUTPUT -m owner --uid-owner "$(id -u gost)" -j ACCEPT

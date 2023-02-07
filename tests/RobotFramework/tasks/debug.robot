###############################################################################
# Debugging Tasks which are meant to assist the developer by spawning a new
# containerized device where they can quickly deploy a new build, or the last
# official build to try to reproduce bugs.
###############################################################################

*** Settings ***
Resource    ../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO    adapter=docker
Library    DebugLibrary

Test Teardown    Get Logs

*** Variables ***
${DPKG_INSTALL_SCRIPT}   cd /setup \
...                      && dpkg -i tedge_0*.deb \
...                      && dpkg -i tedge*mapper*.deb \
...                      && dpkg -i tedge*agent*.deb \
...                      && dpkg -i tedge*watchdog*.deb \
...                      && dpkg -i c8y*configuration*plugin*.deb \
...                      && dpkg -i c8y*log*plugin*.deb \
...                      && dpkg -i tedge*apt*plugin*.deb


*** Tasks ***

Debug device with latest official version
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Debug


Debug device with locally built debian packages (upgrading from last official release)
    ${DEVICE_SN}=                            Setup
    Device Should Exist                      ${DEVICE_SN}
    Install Locally Built Packages
    Log Device Info
    Debug


Debug device with locally built debian packages (no upgrade)
    ${DEVICE_SN}=                            Setup    skip_bootstrap=True
    Install Locally Built Packages           connect=yes
    Device Should Exist                      ${DEVICE_SN}
    Debug


*** Keywords ***

Install Locally Built Packages
    [Arguments]    ${connect}=no
    Transfer To Device                       target/debian/*.deb    /setup/
    Execute Command                          apt-get update && apt-get install -y --no-install-recommends mosquitto
    Execute Command                          ${DPKG_INSTALL_SCRIPT}
    IF    "${connect}" == "yes"
        Execute Command                      test -f ./bootstrap.sh && ./bootstrap.sh --no-install || true
    END

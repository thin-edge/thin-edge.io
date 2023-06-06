*** Settings ***
Documentation       Two settings have been specifically added to tedge-apt-plugin.
...    tedge-apt-plugin list --name $PATTERN lists the packages which name matches the regular expression $PATTERN.
...    tedge-apt-plugin list --maintainer $PATTERN lists the packages which maintainer matches the regular expression $PATTERN.
...    If both --name and --maintainer patterns are provided, tedge-apt-plugin lists packages which either the name or the maintainer matches.
...    These two settings can stored in /etc/tedge/tedge_config.toml in an apt table.
...    If stored in /etc/tedge/tedge_config.toml, then the apt.name and apt.maintainer settings are used to filter packages listed to the cloud.
...    No support is provided by tedge config so store apt.name and apt.maintainer.

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Logs


*** Test Cases ***


tedge-apt-plugin list
    ${apt_list}    Execute Command    /etc/tedge/sm-plugins/apt list
    Should Contain    ${apt_list}    apt
    Should Contain    ${apt_list}    wget
    Should Contain    ${apt_list}    dpkg

tedge-apt-plugin list --name
    ${apt_list_name}    Execute Command    /etc/tedge/sm-plugins/apt list --name '(python|vim).*'
    Should Not Contain    ${apt_list_name}    c8y-configuration-plugin
    Should Not Contain    ${apt_list_name}    c8y-firmware-plugin
    Should Not Contain    ${apt_list_name}    c8y-log-plugin
    Should Not Contain    ${apt_list_name}    c8y-remote-access-plugin
    Should Not Contain    ${apt_list_name}    tedge
    Should Not Contain    ${apt_list_name}    tedge-agent
    Should Not Contain    ${apt_list_name}    tedge-apt-plugin
    Should Not Contain    ${apt_list_name}    tedge-mapper
    Should Not Contain    ${apt_list_name}    tedge-watchdog
    Should Contain    ${apt_list_name}    vim-tiny
    Should Contain    ${apt_list_name}    vim-common

tedge-apt-plugin list --maintainer
    ${apt_list_maintainer}    Execute Command    /etc/tedge/sm-plugins/apt list --maintainer '.*(thin-edge.io).*'
    Should Contain    ${apt_list_maintainer}    c8y-configuration-plugin
    Should Contain    ${apt_list_maintainer}    c8y-firmware-plugin
    Should Contain    ${apt_list_maintainer}    c8y-log-plugin
    Should Contain    ${apt_list_maintainer}    c8y-remote-access-plugin
    Should Contain    ${apt_list_maintainer}    tedge
    Should Contain    ${apt_list_maintainer}    tedge-agent
    Should Contain    ${apt_list_maintainer}    tedge-apt-plugin
    Should Contain    ${apt_list_maintainer}    tedge-mapper
    Should Contain    ${apt_list_maintainer}    tedge-watchdog
    Should Not Contain    ${apt_list_maintainer}    vim-tiny
    Should Not Contain    ${apt_list_maintainer}    vim-common

tedge-apt-plugin list --name --maintainer
    ${apt_list_name_maintainer}    Execute Command    /etc/tedge/sm-plugins/apt list --name '(python|vim).*' --maintainer '.*(raspberry|thin-edge.io).*'
    Should Contain    ${apt_list_name_maintainer}    c8y-configuration-plugin
    Should Contain    ${apt_list_name_maintainer}    c8y-firmware-plugin
    Should Contain    ${apt_list_name_maintainer}    c8y-log-plugin
    Should Contain    ${apt_list_name_maintainer}    c8y-remote-access-plugin
    Should Contain    ${apt_list_name_maintainer}    tedge
    Should Contain    ${apt_list_name_maintainer}    tedge-agent
    Should Contain    ${apt_list_name_maintainer}    tedge-apt-plugin
    Should Contain    ${apt_list_name_maintainer}    tedge-mapper
    Should Contain    ${apt_list_name_maintainer}    tedge-watchdog
    Should Contain    ${apt_list_name_maintainer}    vim-tiny
    Should Contain    ${apt_list_name_maintainer}    vim-common

invalid regex pattern
    Execute Command    /etc/tedge/sm-plugins/apt list --name '(python'    exp_exit_code=1

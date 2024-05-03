*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Force Tags    theme:c8y    theme:benchmarks    \#2326
Suite Setup    Suite Setup
Test Setup    Test Setup

# Note: Don't get logs at the end of each test as the benchmarks cause too much noise

*** Test Cases ***

Publish measurements varying period
    [Template]    Run Benchmark
    count=500    beats=100    beats_delay=0    period=0:25:100
    count=1000    beats=100    beats_delay=0    period=0:25:100

Publish measurements varying count
    [Template]    Run Benchmark
    count=500:250:1000    beats=100    beats_delay=0    period=10

Publish measurements varying beats
    [Template]    Run Benchmark
    count=500    beats=100:100:500    beats_delay=0    period=0

Publish measurements varying beats_delay
    [Template]    Run Benchmark
    count=500    beats=500    beats_delay=1:2:10    period=0


*** Keywords ***

Suite Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}

    ThinEdgeIO.Transfer To Device    ${CURDIR}/benchmark.py         /usr/bin/
    Execute Command    sudo apt-get update && sudo apt-get install -y python3-minimal python3-paho-mqtt --no-install-recommends
    Execute Command    benchmark.py configure

Test Setup
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

Run Benchmark
    [Arguments]    ${count}    ${beats}    ${beats_delay}    ${period}
    Execute Command    benchmark.py run --count ${count} --beats ${beats} --beats-delay ${beats_delay} --period ${period} --pretty --qos 0 --verbose

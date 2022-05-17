#!/bin/bash

# TODO Download not to home but to a separate folder
# TODO Add magic for cross compilation
# TODO Using star as bash expansion does not work here

# TODO Add cargo test (see following lines)
#     nice cargo test
#     cargo test --verbose --no-run --features integration-test
#     cargo test --verbose --features integration-test
#     cargo test --features integration-test
#     cargo test -- --test-threads=1
#     Some will fail when the runner is not started with sudo
#     nice cargo test --verbose --features integration-test,requires-sudo -- --skip sending_and_receiving_a_message
#     nice cargo test --verbose --features integration-test -- --skip sending_and_receiving_a_message

help() {
    echo "
    Setup Tege
    ==========

    A tool to bootstrap and setup Thin-Edge.io.

    Use Cases
      - local : Use self built thin-edge
      - github : Use in context of our GitHub Actions
      - non local : Run on a device that needs to download thin edge from
        a previously built version
    "
    echo "
    Available comands:
    ===================
    - help           : Print help
    - checkvars      : Check necessary environment variables
    - disconnect     : Disconnect all bridges
    - cleanup        : Remove all packages and traces from tedge
    - cleanup_files  : Cleanup downloaded files
    - download       : Download and unpack in the home directory
    - build          : Build tedge locally
    - install_deps   : Install dependencies
    - install  [ local | gihub | home ] :
        local:  Install a locally build version
        github: Install a version that was downloaded by a github action
        home:   Install a version that is located in home

    - getrelase      : Download and install latest release
    - upgrade        : Run apt update and upgrade
    - gitclone       : Clone thin-edge.io into home
    - gitupdate      : Update git repo
    - configure      : Configure tedge
    - configure_collectd : Configure collectd (for NFP analytics)
    - smoketest [ local | github ] :
        use locally build sawtooth publisher or one within a github workflow
    - setupenv       : Prepare all Python environments
    - getid          : Retrieve C8Y ID of the device
    - tedge_help     : Print help
    - systest <test> : Run System test
    - run_local_steps : Run all steps to build and smoketest tedge
    "

    echo "
    Download
    ========
    To download from GitHub call:
    wget https://raw.githubusercontent.com/abelikt/thin-edge.io/continuous_integration/ci/setup_tedge.sh
    chmod +x setup_tedge.sh
    "

}

checkvars() {
    echo "Running function ${FUNCNAME[0]}"
    if [ -z $C8YDEVICE ]; then
        echo "Error: Please supply your device name in variable C8YDEVICE"
        exit 1
    else
        echo "Your device: $C8YDEVICE"
    fi

    if [ -z $C8YUSERNAME ]; then
        echo "Error: Please supply your user name in variable C8YUSERNAME"
        exit 1
    else
        echo "Your user name: $C8YUSERNAME"
    fi

    if [ -z $C8YTENANT ]; then
        echo "Error: Please supply your tennant ID in variable C8YTENANT"
        exit 1
    else
        echo "Your tennant ID: $C8YTENANT"
    fi

    if [ -z $C8YPASS ]; then
        echo "Error: Please supply your c8ypassword in variable C8YPASS"
        exit 1
    fi

    if [ -z $THEGHTOKEN ]; then
        echo "Warning: To download you will need your GitHub Token in THEGHTOKEN"
    fi
}

disconnect() {
    echo "Running function ${FUNCNAME[0]}"

    set +e
    sudo tedge disconnect c8y
    sudo tedge disconnect az
    set -e

    set +e
    sudo systemctl stop tedge-mapper-collectd
    sudo systemctl stop apama
    set -e
}

cleanup() {
    echo "Running function ${FUNCNAME[0]}"

    cd ~/

    # In the hope hat it stops asking
    export DEBIAN_FRONTEND=noninteractive

    sudo -E dpkg -P c8y_configuration_plugin tedge_agent tedge_logfile_request_plugin tedge_mapper tedge_apt_plugin tedge_apama_plugin tedge mosquitto libmosquitto1 collectd-core mosquitto-clients collectd

    # Used by some system tests
    sudo dpkg -P  asciijump robotfindskitten asciijump moon-buggy squirrel3
}

cleanup_files() {
    echo "Running function ${FUNCNAME[0]}"
    cd ~/

    set +f # Enablle pathname expansion
    rm -rf tedge*.deb
    rm -rf sawtooth_publisher
    rm -rf tedge_dummy_plugin
    rm -rf tedge_dummy_plugin_*.deb
    rm -rf sawtooth_publisher_*.deb
    rm -rf debian-packages-*.deb
    rm -rf debian-packages-*.zip
    rm -rf sawtooth_publisher_*.zip
    rm -rf tedge_dummy_plugin_*.zip
    rm -rf c8y_configuration_plugin_*.deb
    set -f
}

download() {

    # TODO Prerequisite:
    # cp /home/micha/Repos/thin-edge.io_qa/GitHub_Artifacts/download_build_artifact.py ~/

    echo "Running function ${FUNCNAME[0]}"

    cd ~/

    ARCH=$(dpkg --print-architecture) # am64 or armhf
    # cd ~/thin-edge.io/ci

    if [ $ARCH == "armhf" ]; then
        echo "armhf"
        ARCH="armv7-unknown-linux-gnueabihf"
    elif [ $ARCH == "amd64" ]; then
        echo "amd64"
    else
        echo "Unknown architecture"
    fi

    ~/download_build_artifact.py abelikt --filter debian-packages-$ARCH

    unzip debian-packages-$ARCH.zip

    ~/download_build_artifact.py abelikt --filter sawtooth_publisher_$ARCH

    unzip sawtooth_publisher_$ARCH.zip

    # TODO check which one is there
    set +e
    chmod +x ~/sawtooth_publisher
    chmod +x /home/pi/examples/sawtooth_publisher
    set -e

    ~/download_build_artifact.py abelikt --filter tedge_dummy_plugin_$ARCH

    # TODO check which one is there
    set +e
    chmod +x ~/tedge_dummy_plugin
    chmod +x ~/tedge_dummy_plugin/tedge_dummy_plugin
    set -e

}

build() {
    echo "Running function ${FUNCNAME[0]}"

    cd ~/thin-edge.io
    JOBS=11

    nice cargo build --release --jobs $JOBS

    nice cargo deb -p tedge
    nice cargo deb -p tedge_agent
    nice cargo deb -p tedge_mapper
    nice cargo deb -p tedge_apt_plugin
    nice cargo deb -p tedge_apama_plugin
    nice cargo deb -p tedge_logfile_request_plugin
    nice cargo deb -p c8y_configuration_plugin

    cd ~/thin-edge.io/crates/tests/sawtooth_publisher

    nice cargo build --jobs $JOBS
}

upgrade() {

    sudo apt update
    sudo apt --assume-yes upgrade
}

install_deps() {
    echo "Running function ${FUNCNAME[0]}"

    #export DEBIAN_FRONTEND=noninteractive
    sudo DEBIAN_FRONTEND=noninteractive apt-get --assume-yes install libmosquitto1 \
        mosquitto mosquitto-clients collectd collectd-core \
        wget curl git python3-venv
}

install() {
    echo "Running function ${FUNCNAME[0]} $1"

    if [ $1 == "local" ]; then
        DIR=~/thin-edge.io/target/debian
    elif [ $1 == "home" ]; then
        DIR=~
    elif [ $1 == "github" ]; then
        DIR="/home/pi/actions-runner/_work/thin-edge.io/thin-edge.io/debian-package_unpack"
    else
        echo "Unknown location"
        exit 1
    fi

    ARCH=$(dpkg --print-architecture) # am64 or
    echo "Architecture is $ARCH".
    export DEBIAN_FRONTEND=noninteractive

    set +f # enable pathname expansion

    # TODO We expect a version with zero here, otherwise the wildcard wouldn't work
    sudo dpkg -i $DIR"/tedge_0"*"_"$ARCH".deb"

    sudo dpkg -i $DIR"/tedge_mapper_"*"_"$ARCH".deb"
    sudo dpkg -i $DIR"/tedge_agent_"*"_"$ARCH".deb"
    sudo dpkg -i $DIR"/tedge_"*"_plugin_"*"_"$ARCH".deb"
    #sudo dpkg -i $DIR"/tedge_apt_plugin_"*"_"$ARCH".deb"
    #sudo dpkg -i $DIR"/tedge_apama_plugin_"*"_"$ARCH".deb"
    #sudo dpkg -i $DIR"/tedge_logfile_request_plugin_"*"_"$ARCH".deb"
    sudo dpkg -i $DIR"/c8y_configuration_plugin_"*"_"$ARCH".deb"
    set -f

}

getrelease() {
    # TODO Clone first
    cd ~/thin-edge.io
    sudo ./get-thin-edge_io.sh
}


gitclone(){
    echo "Running function ${FUNCNAME[0]}"
    cd ~/
    set +e
    git clone https://github.com/abelikt/thin-edge.io
    set -e
}

gitupdate(){
    echo "Running function ${FUNCNAME[0]}"
    cd ~/thin-edge.io

    #git clean -dxf
    git checkout continuous_integration
    git pull abelikt continuous_integration
}

configure_collectd(){
    echo "Running function ${FUNCNAME[0]}"
    sudo cp "/etc/tedge/contrib/collectd/collectd.conf" "/etc/collectd/collectd.conf"
}

configure(){
    echo "Running function ${FUNCNAME[0]}"

    cd ~/thin-edge.io

    ./ci/configure_bridge.sh

    echo "Wait for 5s to give C8y some time to settle"
    sleep 5
}

tedge_help(){
    echo "Running function ${FUNCNAME[0]}"
    tedge --help
}

smoketest() {
    echo "Running function ${FUNCNAME[0]}"

    if [ $1 == "local" ]; then
        # use locally built version
        EXAMPLEDIR=~/thin-edge.io/target/release
    elif [ $1 == "github" ]; then
        # use downloded version from github
        EXAMPLEDIR=/home/pi/examples
    else
        EXAMPLEDIR=~/
    fi

    echo "Will request C8y for the new device ID"
    source ~/env-pysys/bin/activate
    export C8YDEVICEID=$(python3 ~/thin-edge.io/ci/find_device_id.py --tenant $C8YTENANT --user $C8YUSERNAME --device $C8YDEVICE --url $C8YURL)
    deactivate

    echo "New Cumulocity device ID is $C8YDEVICEID"

    sudo tedge connect c8y

    cd ~/thin-edge.io

    echo "Publish some values"
    tedge mqtt pub c8y/s/us 211,20
    sleep 0.1
    tedge mqtt pub c8y/s/us 211,30
    sleep 0.1
    tedge mqtt pub c8y/s/us 211,20
    sleep 0.1
    tedge mqtt pub c8y/s/us 211,30
    sleep 0.1

    echo "Wait some seconds until our 10 seconds window is empty again"
    sleep 12

    echo "Uses SmartREST"
    ./ci/roundtrip_local_to_c8y.py -m REST -pub $EXAMPLEDIR -u $C8YUSERNAME -t $C8YTENANT -id $C8YDEVICEID

    echo "Wait some seconds until our 10 seconds window is empty again"
    sleep 12

    echo "Use thin-edge JSON"
    ./ci/roundtrip_local_to_c8y.py -m JSON -pub $EXAMPLEDIR -u $C8YUSERNAME -t $C8YTENANT -id $C8YDEVICEID

    sudo tedge disconnect c8y
}

setupenv() {
    echo "Running function ${FUNCNAME[0]}"

    python3 -m venv ~/env-pysys
    source ~/env-pysys/bin/activate
    pip3 install -r ~/thin-edge.io/tests/requirements.txt
    deactivate


    python3 -m venv ~/env-c8y-api retry-decorator
    source ~/env-c8y-api/bin/activate
    pip3 install c8y-api retry-decorator
    deactivate
}

getid() {
    echo "Running function ${FUNCNAME[0]}"

    echo "Will request C8y for the new device ID"
    . ~/env-c8y-api/bin/activate
    export C8YDEVICEID=$(python3 ~/thin-edge.io/ci/find_device_id.py \
            --tenant $C8YTENANT --user $C8YUSERNAME \
            --device $C8YDEVICE --url $C8YURL)
    echo "Cumulocity ID is $C8YDEVICEID"
    deactivate
}

systest() {
    echo "Running function ${FUNCNAME[0]}"

    getid;

    cd ~/thin-edge.io
    source ~/env-pysys/bin/activate

    cd ~/thin-edge.io/tests/PySys;


    echo "$C8YDEVICEID"

    #pysys.py run apama_plugin_install -XmyPlatform=container
    #pysys.py run sm_apt_install_fail  -XmyPlatform='container'
    #pysys.py run -XmyPlatform='container' sm_apt_install_download_path

    pysys.py run -XmyPlatform='container' $1

    deactivate
}

run_local_steps() {
    echo "Running function ${FUNCNAME[0]}"

    checkvars;


    disconnect;
    cleanup;
    gitupdate;
    build;
    install_deps;
    install local;
    tedge_help;
    setupenv;
    configure_collectd;
    configure;
    smoketest local;
}

test() {
    echo $1
    echo $@
}

#set -x

# Harsh mode we exit when an error occurs. Might cause some trouble when
# stuff is only half configured.
# Unset this if an error is allowed to happen
set -e

if [ $1 != "help" ] ; then
    checkvars;
fi

#. ~/env.sh

$1 $2

echo "Done"



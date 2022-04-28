


help() {
    echo "Available commands:"
    echo "help"
    echo "checkvars"
    echo "disconnect"
    echo "cleanup"
    echo "download"
    echo "build_local"
    echo "install_deps"
    echo "install"
    echo "install_local"
    echo "gitupdate"
    echo "gitclone"
    echo "configure"
    echo "configure_collectd"
    echo "smoketest"
    echo "setupenv"
    echo "getid"
    echo "tedge_help"
    echo "systest"
    echo "run_local_steps"

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

    sudo tedge disconnect c8y
    sudo tedge disconnect az
    rm -f ~/*.deb
    rm -f ~/thin-edge.io/target/debian/*.deb

    set +e
    sudo systemctl stop tedge-mapper-collectd
    sudo systemctl stop apama
    set -e
}

cleanup() {
    echo "Running function ${FUNCNAME[0]}"
    # In the hope hat it stops asking
    export DEBIAN_FRONTEND=noninteractive
    sudo dpkg -P c8y_configuration_plugin tedge_agent tedge_logfile_request_plugin tedge_mapper tedge_apt_plugin tedge_apama_plugin tedge mosquitto libmosquitto1 collectd-core mosquitto-clients collectd
    rm -rf debian-packages-armv7-unknown-linux-gnueabihf.zip
    rm -rf sawtooth_publisher_armv7-unknown-linux-gnueabihf.zip
    rm -rf tedge*.deb
    rm -f sawtooth_publisher
    rm -f debian-packages-*
}

download() {
    echo "Running function ${FUNCNAME[0]}"

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

    ./download_build_artifact.py abelikt --filter debian-packages-$ARCH

    unzip debian-packages-$ARCH.zip

    ./download_build_artifact.py abelikt --filter sawtooth_publisher_$ARCH

    unzip sawtooth_publisher_$ARCH.zip

    chmod +x /home/pi/examples/sawtooth_publisher

    ./download_build_artifact.py abelikt --filter tedge_dummy_plugin_$ARCH

    chmod +x /home/pi/tedge_dummy_plugin/tedge_dummy_plugin

}

build_local() {
    echo "Running function ${FUNCNAME[0]}"

    cd ~/thin-edge.io

    nice cargo build --release --jobs 11

    nice cargo deb -p tedge
    nice cargo deb -p tedge_agent
    nice cargo deb -p tedge_mapper
    nice cargo deb -p tedge_apt_plugin
    nice cargo deb -p tedge_apama_plugin
    nice cargo deb -p tedge_logfile_request_plugin

    cd ~/thin-edge.io/crates/tests/sawtooth_publisher

    nice cargo build --jobs 11
}

install_deps() {
    echo "Running function ${FUNCNAME[0]}"

    export DEBIAN_FRONTEND=noninteractive
    sudo apt-get --assume-yes install libmosquitto1 mosquitto mosquitto-clients collectd collectd-core
}

install() {
    echo "Running function ${FUNCNAME[0]}"

    ARCH=$(dpkg --print-architecture) # am64 or
    echo "Architecture is $ARCH".
    export DEBIAN_FRONTEND=noninteractive
    sudo dpkg -i ~/tedge_0.*_$ARCH.deb
    sudo dpkg -i ~/tedge_mapper_*_$ARCH.deb
    sudo dpkg -i ~/tedge_agent_*_$ARCH.deb
    sudo dpkg -i ~/tedge_*_plugin_*_$ARCH.deb
}

install_local() {
    echo "Running function ${FUNCNAME[0]}"

    ARCH=$(dpkg --print-architecture) # am64 or armhf
    echo "Architecture is $ARCH"
    export DEBIAN_FRONTEND=noninteractive
    sudo dpkg -i ~/thin-edge.io/target/debian/tedge_0.*_$ARCH.deb
    sudo dpkg -i ~/thin-edge.io/target/debian/tedge_mapper_*_$ARCH.deb
    sudo dpkg -i ~/thin-edge.io/target/debian/tedge_agent_*_$ARCH.deb
    sudo dpkg -i ~/thin-edge.io/target/debian/tedge_*_plugin_*_$ARCH.deb
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
    git pull origin continuous_integration
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


    python3 -m venv ~/env-c8y-api
    source ~/env-c8y-api/bin/activate
    pip3 install c8y-api
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

    cd thin-edge.io
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

    /home/micha/Project-SAG/Scripts/setup_tedge.sh checkvars

    #echo "Testing with cargo test"
    # nice cargo test
    # cargo test --verbose --no-run --features integration-test
    # cargo test --verbose --features integration-test
    # cargo test --features integration-test
    # cargo test -- --test-threads=1
    # Some will fail when the runner is not started with sudo
    # nice cargo test --verbose --features integration-test,requires-sudo -- --skip sending_and_receiving_a_message
    # nice cargo test --verbose --features integration-test -- --skip sending_and_receiving_a_message

    ~/thin-edge.io/ci/setup_tedge.sh disconnect
    ~/thin-edge.io/ci/setup_tedge.sh cleanup
    ~/thin-edge.io/ci/setup_tedge.sh gitclone

    ~/thin-edge.io/ci/setup_tedge.sh build_local
    #~/thin-edge.io/ci/setup_tedge.sh download

    ~/thin-edge.io/ci/setup_tedge.sh install_deps

    #~/thin-edge.io/ci/setup_tedge.sh install
    ~/thin-edge.io/ci/setup_tedge.sh install_local

    ~/thin-edge.io/ci/setup_tedge.sh tedge_help
    ~/thin-edge.io/ci/setup_tedge.sh setupenv
    ~/thin-edge.io/ci/setup_tedge.sh configure_collectd
    ~/thin-edge.io/ci/setup_tedge.sh configure

    /home/micha/Project-SAG/Scripts/setup_tedge.sh smoketest local
}

test() {
    echo $1
    echo $@
}

#set -x

# Harsh mode we exit when an error occurs
# unset this if an error is allowed to happen
set -e

checkvars;

#. ~/env.sh

$1 $2

echo "Done"



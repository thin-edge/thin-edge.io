#!/usr/bin/sh

# Still clumsy integration of all ci steps

# a simple checker function
check() {
    if [ $? -ne 0 ]; then
        echo "Error: Exiting due to previous error"
        exit 1;
    fi
}

appendtofile() {
    STRING=$1
    FILE=$2
    if grep "$STRING" $FILE; then
        echo 'line already there';
    else
        echo $STRING >> $FILE;
    fi
}

set -x

# Binary needs to be precompiled and moved to the target later on
DELAY=10
ITERATIONS=100
HEIGHT=100

# we need to wait for them to end!
# /home/pi/thin-edge.io/target/debug/examples/sawtooth_publisher $DELAY $HEIGHT $ITERATIONS 200,value_a,T &
# /home/pi/thin-edge.io/target/debug/examples/sawtooth_publisher $DELAY $HEIGHT $ITERATIONS 200,value_b,T &
# /home/pi/thin-edge.io/target/debug/examples/sawtooth_publisher $DELAY $HEIGHT $ITERATIONS 200,value_c,T &
# /home/pi/thin-edge.io/target/debug/examples/sawtooth_publisher $DELAY $HEIGHT $ITERATIONS 200,value_d,T &
# /home/pi/thin-edge.io/target/debug/examples/sawtooth_publisher $DELAY $HEIGHT $ITERATIONS 200,value_e,T &

seq 5 | parallel -j5  $HOME/thin-edge.io/target/debug/examples/sawtooth_publisher $DELAY $HEIGHT $ITERATIONS 200,value_{},T

check

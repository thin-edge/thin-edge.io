. get-credentials.sh

DEVICE=$1
DOCKER=no

if [ -z "$DEVICE" ]
then
    echo usage: $0 device-identifier
    echo
    echo Configure mosquitto to use this device certificate.
    exit 1
fi
if [ ! -f $DEVICE.crt ]
then
    echo Certificate file $DEVICE.crt not found.
    exit 1
fi
if [ ! -f $DEVICE.key ]
then
    echo Private key file $DEVICE.key not found.
    exit 1
fi

ln -f $DEVICE.key edge.key 
ln -f $DEVICE.crt edge.crt 

if [ $DOCKER = "yes" ]
then
    LOG="file /app/mosquitto.log"
    KEYS=/keys
    APP=/app
    BIND_ADDRESS="# bind_address 127.0.0.1 is set on 'docker run' with '-p 127.0.0.1:1883:1883'"
else
    LOG=stderr
    KEYS=$PWD
    APP=/tmp
    BIND_ADDRESS="bind_address 127.0.0.1"
fi

cat >mosquitto.conf <<EOF
# Only local connections are accepted. No authentication is required.
$BIND_ADDRESS
allow_anonymous true

# Logs
log_dest $LOG
log_type debug
log_type error
log_type warning
log_type notice
log_type information
log_type subscribe         # log subscriptions
log_type unsubscribe
connection_messages true   # log connections and disconnections

# Connection, subscription and message data are written to the disk in $APP/mosquitto.db
persistence true
persistence_location $APP/
persistence_file mosquitto.db
autosave_interval 60          # saved every minute

# Tune for no data-loss, throughput and low memory usage, not for high-concurrency
max_connections       10
max_inflight_messages 5   # per client
max_queued_messages   20  # per client

# C8Y Bridge
connection edge_to_c8y
address mqtt.$C8Y:8883
bridge_cafile $KEYS/c8y-trusted-root-certificates.pem
remote_clientid $DEVICE
bridge_certfile $KEYS/edge.crt
bridge_keyfile $KEYS/edge.key
try_private false
start_type automatic

### Registration
topic s/dcr in 2 c8y/ "" 
topic s/ucr out 2 c8y/ "" 

### Templates
topic s/dt in 2 c8y/ "" 
topic s/ut/# out 2 c8y/ "" 

### Static templates
topic s/us out 2 c8y/ ""
topic t/us out 2 c8y/ ""
topic q/us out 2 c8y/ ""
topic c/us out 2 c8y/ ""
topic s/ds in 2 c8y/ ""
topic s/os in 2 c8y/ ""

### Debug
topic s/e in 0 c8y/ ""

### SmartRest 2.0
topic s/uc/# out 2 c8y/ ""
topic t/uc/# out 2 c8y/ ""
topic q/uc/# out 2 c8y/ ""
topic c/uc/# out 2 c8y/ ""
topic s/dc/# in 2 c8y/ ""
topic s/oc/# in 2 c8y/ ""
EOF

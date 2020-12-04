C8Y_URL=$1
DEVICE_ID=$2
DEVICE_CERT=$3
DEVICE_KEY=$4

if [ -z "$C8Y_URL" -o -z "$DEVICE_ID" -o -z "$DEVICE_CERT" -o -z "$DEVICE_KEY" -o "$#" -ne 4 ]
then
    echo usage: $0 C8Y_URL DEVICE_ID DEVICE_CERT DEVICE_KEY
    echo
    echo Configure mosquitto to use the given certificate to connect c8y.
    exit 1
fi

if [ ! -f $DEVICE_CERT ]
then
    echo Certificate file $DEVICE_CERT not found.
    exit 1
fi
if [ ! -f $DEVICE_KEY ]
then
    echo Private key file $DEVICE_KEY not found.
    exit 1
fi
if ! (file $DEVICE_CERT | grep -q PEM)
then
    echo "[ERROR] The file $DEVICE_CERT is not a certificate: $(file $DEVICE_CERT)"
    exit 1
fi
if !(openssl x509 -in $DEVICE_CERT -noout -subject | grep -q $DEVICE_ID)
then
    echo "The certificate $DEVICE_CERT doesn't match the identifier $DEVICE_ID."
    exit 1
fi

C8Y_CERT=$PWD/c8y-trusted-root-certificates.pem
LOG=stdout
DATA=/tmp

cat >mosquitto.conf <<EOF
# Only local connections are accepted. No authentication is required.
bind_address 127.0.0.1
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

# Connection, subscription and message data are written to the disk in $DATA/mosquitto.db
persistence true
persistence_location $DATA/
persistence_file mosquitto.db
autosave_interval 60          # saved every minute

# Tune for no data-loss, throughput and low memory usage, not for high-concurrency
max_connections       10
max_inflight_messages 5   # per client
max_queued_messages   20  # per client

# C8Y Bridge
connection edge_to_c8y
address mqtt.$C8Y_URL:8883
bridge_cafile $C8Y_CERT
remote_clientid $DEVICE_ID
bridge_certfile $DEVICE_CERT
bridge_keyfile $DEVICE_KEY
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

#!/usr/bin/env bash
C8Y_URL=$1
DEVICE_ID=$2
CERT_PATH=$3
KEY_PATH=$4

if [ -z "$C8Y_URL" -o -z "$DEVICE_ID" -o -z "$CERT_PATH" -o -z "$KEY_PATH" -o "$#" -ne 4 ]
then
    echo "usage: $0 C8Y_URL DEVICE_ID CERT_PATH KEY_PATH"
    echo
    echo "Configure mosquitto to use the given certificate to connect c8y."
    exit 1
fi

if [ ! -f "$CERT_PATH" ]
then
    echo "Certificate file $CERT_PATH not found."
    exit 1
fi
if [ ! -f "$KEY_PATH" ]
then
    echo "Private key file $KEY_PATH not found."
    exit 1
fi
if ! (file "$CERT_PATH" | grep -q PEM)
then
    echo "[ERROR] The file $CERT_PATH is not a certificate: $(file $CERT_PATH)"
    exit 1
fi
if !(openssl x509 -in $CERT_PATH -noout -subject | grep -q "subject=CN = $DEVICE_ID,")
then
    echo "The certificate $CERT_PATH doesn't match the identifier $DEVICE_ID."
    exit 1
fi

DIR=$(dirname $0)
C8Y_CERT=$DIR/c8y-trusted-root-certificates.pem
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
bridge_certfile $CERT_PATH
bridge_keyfile $KEY_PATH
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

### JSON
topic measurement/measurements/create out 2 c8y/ ""
EOF

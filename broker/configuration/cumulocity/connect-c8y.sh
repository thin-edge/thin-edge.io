. get-credentials.sh

DEVICE=$1

if [ -z "$DEVICE" ]
then
    echo usage: $0 device-identifier
    echo
    echo Connect this device to the Cumulocity.
    echo - create a device certificate
    echo - enable this certificate on Cumulocity
    echo - configure the MQTT endpoint

    exit 1
fi

CERT_FILE=$DEVICE.crt
if [ ! -f $CERT_FILE ]
then
    echo "Creating the certificate"
    ./create-self-signed-certificate.sh $DEVICE
fi
if [ -f $CERT_FILE ]
then
    if (file $CERT_FILE | grep -q PEM)
    then
       echo "[OK] The device certificate is stored in $CERT_FILE"
    else
       echo "[ERROR] The file $CERT_FILE is not a certificate: $(file $CERT_FILE)"
       exit 1
    fi
else
   echo "[ERROR] No device certificate has been created"
   exit 1
fi

echo "Creating mosquitto.conf"
./create-mosquitto-conf.sh $DEVICE

echo "Uploading the certificate on Cumulocity"
./upload-certificate.sh $DEVICE

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

if [ ! -f $DEVICE.key ]
then
    echo "Creating the certificate"
    ./create-self-signed-certificate.sh $DEVICE
fi

echo "Creating mosquitto.conf"
./create-mosquitto-conf.sh $DEVICE

echo "Uploading the certificate on Cumulocity"
./upload-certificate.sh $DEVICE

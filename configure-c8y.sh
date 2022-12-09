#!/bin/sh
set -e

if [ "$(id -u)" != 0 ]; then
    printf "Please run as sudo or root!\n"
    exit
fi



printf "\n*********************************************"
printf "\nThis script lets you setup a thinEdge connection to cumulocity IoT.\nWe assume that the installation was successful and all the necessary services are running.
Just follow the instructions....."
printf "\n*********************************************"

printf "\n\n"
read -r -p 'Enter the Cumulocity tenant url e.g. [your-tenant.cumulocity.com]: ' tenant_url
tenant_url=${tenant_url:-your-tenant.cumulocity.com}
tedge config set c8y.url "$tenant_url"

printf "\n\n"
read -r -p 'Enter the path to your root certificates. On most Linux systems that is [/etc/ssl/certs]: ' path_root_cert
path_root_cert=${path_root_cert:-/etc/ssl/certs}
tedge config set c8y.root.cert.path "$path_root_cert"

printf "\n\n"
read -r -p 'Enter your device-id [my-device]: ' device_id
device_id=${device_id:-my-device}
{
    tedge cert create --device-id "$device_id"
} || {
printf "\n\n"
read -r -p "Would you like to remove the old cert and create a new one? [y/N]: " response
case $response in
    [yY]) 
        printf "\nRemoving cert...."
        tedge cert remove
        tedge cert create --device-id "$device_id"
        ;;
    *) 
        printf "\nOk bye, enjoy thingEdge.io!!!\n" 
        exit 
        ;;    
esac
}

printf "\n\nYour current configuration:\n"
tedge config list


RET=1
until [ ${RET} -eq 0 ]; do
    printf "\n\n"
    read -r -p "Enter Cumulocity user [your@email.com]: " c8y_Username
    c8y_Username=${c8y_Username:-your@email.com}
    tedge cert upload c8y --user "$c8y_Username"
    printf "\nWait a bit, c8y has to activated the cert first."
    sleep 10s
    RET=$?
done


printf '\nCreated Certificate:'
sudo tedge cert show


{
    printf "\nConnecting...."
    tedge connect c8y
} || {
        printf "\n\n"
        read -p "Looks like tedge is already connected to Cumulocity. Would you like to reconnect? [y/N]: " response
        case $response in
            [yY])  
                tedge disconnect c8y
                tedge connect c8y
                ;;
            *)  
                printf "\nOk bye, enjoy thingEdge.io!!!\n" 
                exit 
                ;;    
        esac
}

printf "\n\n"
read -r -p 'Would you like to send a test measurement to your new device? [y/N]: ' response
case $response in
    [yY])  
        printf "\nCreating a 20 degree Temperature Measurement...."
        tedge mqtt pub c8y/s/us 211,20
        printf "\nYou should now see a new measurement in your device %s. Enjoy....\n" "$device_id"
        ;;
    *)  
        printf "\nOk bye, enjoy thingEdge.io!!!\n" 
        exit 
          
esac

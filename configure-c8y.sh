#!/bin/sh

printf "\n*********************************************"
printf "\nThis script lets you setup a thinEdge connection to cumulocity IoT.\nWe assume that the installation was successful and all the necessary services are running.
Just follow the instructions....."
printf "\n*********************************************"


printf "\n\n"
read  -p 'Enter the Cumulocity tenant url e.g. [your-tenant.cumulocity.com]: ' tenant_url
tenant_url=${tenant_url:-mstoffel.eu-latest.cumulocity.com}
tedge config set c8y.url $tenant_url

printf "\n\n"
read -p 'Enter the path to your root certificates [/etc/ssl/certs]: ' path_root_cert
path_root_cert=${path_root_cert:-/etc/ssl/certs}
tedge config set c8y.root.cert.path $path_root_cert

printf "\n\n"
read -p 'Enter your device-id [my-device]: ' device_id
device_id=${device_id:-my-device}
{
    tedge cert create --device-id $device_id
} ||
printf "\n\n"
read -r -p "\n\nWould you like to remove the old cert and create a new one? (y)Yes/(n)No: " response
case $response in
    [yY]) 
        printf "\nRemoving cert...."
        tedge cert remove
        tedge cert create --device-id $device_id
        ;;
    *) 
        printf "\nOk bye, enjoy thingEdge.io!!!" 
        exit 
        ;;    
esac


printf "\n\nYour current configuration:\n"
tedge config list


printf "\n\n"
read -p "\n\nEnter Cumulocity user [your@email.com]: " c8y_Username
c8y_Username=${c8y_Username:-your@email.com}

#read -ps 'Enter Cumulocity password [secret]: ' c8y_Password
#c8y_Password=${c8y_Username:-secret}

printf "\n\nUsing $c8y_Username to authenticate...."
tedge cert upload c8y --user $c8y_Username

printf '\nCreated Certificate:'
sudo tedge cert show


{
    printf "\nConnecting...."
    tedge connect c8y
} ||


printf "\n\n"
read -r -p "\nLooks like tedge is already connected to Cumulocity. Would you like to reconnect? (y)Yes/(n)No: " response
case $response in
    [yY])  
        tedge disconnect c8y
        tedge connect c8y
        ;;
    *)  
        printf "\nOk bye, enjoy thingEdge.io!!!" 
        exit 
        ;;    
esac

printf "\n\n"
read -r -p $'Would you like to send a test measurement to your new device? (y)Yes/(n)No: ' response
case $response in
    [yY])  
        printf "\nCreating a 20 degree Temperature Measurement...."
        tedge mqtt pub c8y/s/us 211,20
        printf "\nYou should now see a new measurement in your device. Enjoy...."
        ;;
    *)  
        printf "\nOk bye, enjoy thingEdge.io!!!" 
        exit 
        ;;    
esac

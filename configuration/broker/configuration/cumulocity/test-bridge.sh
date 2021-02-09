#!/usr/bin/env bash
#----------------------------------------------------------------
# Test a bi-directionel chanel is established with c8y over MQTT
#----------------------------------------------

ACTUAL_ERROR=$(
    # Wait for a single error message at most 3 seconds
    mosquitto_sub -C 1 -W 3 --topic c8y/s/e 2>&1 &

    # In parallel wait a bit, then send a dummy smart-rest template
    sleep 1
    mosquitto_pub --topic c8y/s/us --message "999,foo bar" 2>/dev/null
)
EXPECTED_ERROR="40,999,No static template for this message id"

if [ "$ACTUAL_ERROR" = "$EXPECTED_ERROR" ]
then
   echo "[OK] sending and receiving data to and from c8y"
else
   if [ -z "$ACTUAL_ERROR" ]
   then
       echo "[ERROR] fail to get a response for a message sent to c8y"
       echo "        Is the error topic s/e replicated over the bridge?"
   else
       echo "[ERROR] unexpected error: $ACTUAL_ERROR"
       echo "        Is the bridge running?"
   fi
   exit 1
fi

# How to configure a port in mosquitto?

Configuring a mosquitto port in thin edge is a two step process.

# Step 1: Update the port number in mosquitto.conf
 The new port number has to be updated in the mosquitto.conf to inform the mosquitto 
 daemon to use the new port. Add the below line in the mosquitto.conf file.
  
 listener [port number]
    
 For example: `listener 1024` means the mosquitto daemon uses port number 1024 to serve the requests.
 
 Restart the mosquitto to use the new port number that is cofigured.

# Step 2: Inform about the new port to mqtt clients 
 Once the mosquitto daemon is configured to use a specific port. This has to be notified 
 to all the mqtt clients that are using mosquitto to publish/subscrbe.
 Follow below steps to do that
   
 ```shell
   
  `tedge config set mosquitto.port 1024`

 ```
 This will make sure that all the mqtt clients use the newer port that has been set.

# Verify the port configured/set
 Use below command to check the port has been set properly or not.
 
 ```shell
   
  `tedge config get mosquitto.port`

 ```
 This prints out the port that has been set.

# Update to use default port
 To use the default port, one has to remove the `listener [port-number]` from the mosquitto.conf manually.
 Also has to unset the mosquitto.port using the tedge cli framework as below

 ```shell
   
  `tedge config unset mosquitto.port`

 ```

# Error case

```shell
   
   `tedge config set mosquitto.port '"1024"'`

```
   Error case

   Error: failed to set the configuration key: mosquitto.port with value: "1234".

Caused by:
    Conversion from String failed

   


  


FROM alpine:3.12.0
RUN apk add mosquitto
RUN apk add mosquitto-clients

EXPOSE 1883/tcp
EXPOSE 8883/tcp

RUN mkdir -p /keys
COPY c8y-io-chain.pem /keys/c8y-io-chain.pem
COPY edge.crt /keys/edge.crt
COPY edge.key /keys/edge.key
RUN chown mosquitto /keys/*

RUN mkdir -p /app
RUN chown mosquitto /app

COPY mosquitto.conf /mosquitto.conf

CMD ["mosquitto", "-c", "/mosquitto.conf"]

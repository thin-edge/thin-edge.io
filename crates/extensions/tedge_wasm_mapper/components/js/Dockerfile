FROM denoland/deno:latest
RUN deno install npm:@bytecodealliance/componentize-js
WORKDIR /wasm
COPY componentize.mjs .
VOLUME ["/wasm/src"]

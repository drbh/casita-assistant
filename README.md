# casita-assistant

very lite open source home automation.

*casita* means "little house" in Spanish.

## Design Goals
- super lightweight; no bloat
- riscv targeted
- buildable with minimal dependencies
- easily connect devices and add automations
- support zigbee devices, rstp cameras, and mjpeg camera feeds

## Running

```bash
cargo build --release --features embed-frontend
```

its recommended to setup a `/etc/systemd/system/casita-assistant.service` file to run it as a service so it restarts on boot/serial port changes.

## Motivation

Casita Assistant is a minimalistic home automation tool that I built for my own use.

The core motivator was that I have nm orange pi 5 RV2 that I wanted to use as a home automation server, but getting Home Assistant to run natively was a nightmare. HA installed many dependencies I did not need/want like AI frameworks and complex integration systems.

So I just thought i'd roll my own smaller home automation server that does all of the things I need and I can slowly add more features as my system grows.

## Notes

- currently the webapp is bundled and included in source so only the backend needs to be compiled to get started.
- the core focus is zigbee support via a conbee 2 stick.
- you may need to specify `CONBEE_PORT="..."` environment variable to point to the correct serial port for the dongle.
- only tested/running daily with Orange Pi 5 RV2 running ubuntu and a conbee 2 stick.

# Matrix Calendar Bot

A bot that responds to '!cal' and '!calendar' events in specified Matrix rooms, fetches calendar events for the next 7 days from a CalDAV server, and posts them in the room.

This repo is structured as an Ansible role which could go inside the 'custom' directory of [matrix-docker-ansible-deploy](https://github.com/spantaleev/matrix-docker-ansible-deploy) or some other Ansible playbook.

It's a Rust app which is compiled and run on the server in a Docker container and managed with systemd.

Some code is adapted from [kitchen_fridge](https://github.com/daladim/kitchen-fridge) and [matrix-rust-sdk example code](https://github.com/matrix-org/matrix-rust-sdk/blob/main/examples/persist_session/src/main.rs).

# Introduction

This repo contains:

- Server config and install scripts
    - `server/nixos` is the NixOS config
- Templates for CI runner images
    - `profiles/servo-windows10/*` is for **Windows 10** runners
    - `profiles/servo-ubuntu2204/*` is for **Ubuntu 22.04** runners
    - `profiles/servo-macos13/*` is for **macOS 13** runners
    - `profiles/servo-macos14/*` is for **macOS 14** runners
    - `profiles/servo-macos15/*` is for **macOS 15** runners
- A service that automates runner management
    - `monitor` is the service
    - `.env.example` and `monitor.toml.example` contain the settings


# TKS

![CI Status](https://github.com/linux-tks/tks/actions/workflows/rust-test.yml/badge.svg)

TKS Keeps Secrets, otherwise called "Tux Keeps Secrets", pronounced the same
as "ticks". It is built around the [Secret Service
API](https://specifications.freedesktop.org/secret-service-spec/0.2/description.html), which is a standard by the FreeDesktop.org project. When present on user's system, it will be instantly recognized by standard Linux applications such as `Network Manager`, `Google Chrome` and so on.

## Installation

### From Package

This project is not yet packaged for any distribution. If you want to package it for your distribution, please let me
know. For now, you can install it from source.

### From Source

This project is still in early development, so it is not yet available in any package manager. To install it from source, please follow the instrunctions given in the [CONTRIBUTING.md](CONTRIBUTING.md) file.

## Aim

This project aims to provide a secure way to store secrets on a Linux system.
It is designed to be used in a per user manner. Main design goals are:
- Secure
- Easy to use from the command line
- Easy to use from scripts
- Easy to use from other programs
- No dependencies on any Desktop Environment
- As close to zero configuration as possible

This project is inspired by the problems I've encountered when I was
maintaining KDE's KWallet system. I've also been inspired by other password
managers.

## Design

See [TKS Architecture](doc/architecture.rst)


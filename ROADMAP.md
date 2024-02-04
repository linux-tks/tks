
# TKS Versioning Scheme

TKS release versioning uses the format "M.F.B" where:
- M is the milestone number of the release
- F is the milestone feature set
- B is the bugfix number

There are no development releases. The master branch should be used in case
one needs the very latest.

# Milestones

Below is the list of milestones and their status.

NOTE: first release is yet to be done.

## Milestone 0

- [✔] Create a repository for the project
- [✔]] Create project structure PoC

## Milestone 1

- [✔] org.freedesktop.secrets interface specification is implemented by
  tks-service; storage in non-encrypted files

### Milestone 1.1

- [x] Yubikey-based encryption is implemented

### Milestone 1.2

- [x] Importing of the secrets from KWallet is implemented

### Milestone 1.3

- [x] Importing of the secrets from GNOME Keyring is implemented

### Milestone 1.4

- [x] Importing of the secrets from Pass is implemented

### Milestone 1.5

- [x] Importing of the secrets from KeePassX is implemented

## Milestone 2

- [x] storage in encrypted files using TPM 2.0 infrastructure

### Milestone 2.1

- [x] Automatic unlocking using the PAM module is implemented


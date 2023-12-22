
# TKS Versioning Scheme

TKS versioning uses the format "YYYY.MM-X" where:
- YYYY is the year of the release
- MM is the month of the release
- X is the milestone number of the release

Development versions are named "YYYY.MM-X-N" where:
- YYYY is the year of the release
- MM is the month of the release
- X is the milestone number of the release
- N is the number of the development version, that is, a sequence number
  that is incremented for each development version

# Milestones

## Milestone 0

- [✔] Create a repository for the project
- [✔]] Create project structure PoC

## Milestone 1

- [x] org.freedesktop.secrets interface specification is implemented by
  tks-service; storage in non-encrypted files

## Milestone 2

- [x] org.freedesktop.secrets interface specification is implemented by
  tks-service; storage in encrypted files using TPM 2.0 infrastructure

## Milestone 3

- [x] tks-client is implemented


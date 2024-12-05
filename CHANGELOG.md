# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2024-12-05

### Changed (Breaking)

- *(build)* Bump alloy to v0.7.12

## [0.2.0] - 2024-08-06

### Features

- *(deploy)* Return deployment transaction receipt (#9)


## [0.1.2] - 2024-07-15

### Bug Fixes

- Stop printing activation fee


## [0.1.1] - 2024-07-15

### Bug Fixes

- Set infinite balance when computing activation fee


### Features

- Add support for specifying no solidity constructor
- Add -q cli flag to suppress console output


## [0.1.0] - 2024-06-18

### Bug Fixes

- *(README)* Fix typo
- *(README)* Update wording & example output
- *(docs)* Update WARNING & add Limitations section to README
- *(docs)* Update installation instructions
- Compute shifts properly
- Amend static jumps
- Properly tokenize nested calls
- Properly handle contracts having been activated
- Typo in README
- Expect abi-encoded args instead of vec
- Not init a runtime in exported fns
- Use default data fee with --deploy-only


### Features

- *(deploy)* Add --deploy-only cli flag
- *(docs)* Add README.md
- *(docs)* Improve README.md
- *(docs)* Explain the meaning of koba
- *(docs)* Add installation steps to README
- *(docs)* Add deploy command to README
- Revamp implementation to use evmasm (#1)
- Test tokenizer, labeler & compiler
- Properly handle nested function calls
- Add a deploy command
- Return deployed contract address
- Add support for MCOPY
- Add support for Stylus Testnet v1
- Add motivation section to README.md


### Miscellaneous Tasks

- Update crate authors


### Build

- Use the published version of alloy

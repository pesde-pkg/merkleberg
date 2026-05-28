# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-05-28

## Added

- Exported `MMRBatch` as a public type since it is returned by public methods
- `MMRBatch::into_store` method to consume the batch and return an owned store
- Additional Cargo metadata with links for homepage and doc site

## Fixed

- Broken links in docs to various resources and types
- `std` usage being incorrectly gated, hence the crate not working with it disabled
- Internal benchmarks, tests and CI pipelines

## [0.1.0] - 2026-05-20

Initial library release.

[unreleased]: https://github.com/pesde-pkg/merkleberg/commits/HEAD
[0.1.0]: https://crates.io/crates/merkleberg/0.1.0
[0.2.0]: https://crates.io/crates/merkleberg/0.20

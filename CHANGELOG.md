# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking Changes

- **`--ipc` flag removed**: The `--ipc` flag is no longer recognized. IPC is now the default.
  - Migration: Remove `--ipc` from scripts; it is no longer needed.
  - To opt out of IPC (direct SQLite): use `--no-ipc` instead.

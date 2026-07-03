# Security Policy

<!--
    File: Security policy.
    Intent: Explain how to report security issues and what the tool does with local data.
-->

## Supported Versions

This project is pre-1.0. Security fixes are made on the default branch.

## Reporting a Vulnerability

Please do not open a public issue for vulnerabilities that expose sensitive local data.

If this repository has GitHub Security Advisories enabled, use a private advisory. Otherwise, contact the maintainer by
the channel listed on the GitHub repository.

## Local Data

`electron-detector` reads local process metadata and NTFS file metadata. It writes a local cache to:

```text
%LOCALAPPDATA%\electron-detector\cache.json
```

The cache can contain local application paths. Treat it as local diagnostic data.

The tool does not send telemetry or network requests.

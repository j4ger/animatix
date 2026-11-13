# Canonical SPDX references

These are standard reference texts from the pinned SPDX License List release
recorded in `manifest.json`. They are not copies of license files from the
listed Rust packages. Upstream package archives declare SPDX expressions but
do not contain their own full license texts.

The manifest binds each reference to the exact package version, registry
checksum, source revision, declared expression, repository, and author metadata.
It also records the official reference URL and SHA-256 of each unchanged text.
Copyright placeholders remain unchanged; author metadata is not asserted to be
a copyright notice.

The SDK notice generator must keep these references separate from verified
upstream license files. Including a reference does not clear an upstream-text
missing flag or establish distribution sufficiency. Replace this fallback with
verified package-specific material when upstream provides it.

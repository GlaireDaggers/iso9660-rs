## Joliet

* [x] Decode all strings in the table per character encoding
* [x] Dir iterator should use supp table
* [x] Rename CharacterEncoding::parse to something more friendly
* [ ] Sort out utf-16 error handling

## Rockridge

* [x] Root selection
* [ ] API for POSIX attributes and timestamps
* [ ] Parse ER and RR records to identify all extensions that are present. Expose API.
* [ ] Handle symlinks
* [ ] Refactor and clean up module hierarchy
* [ ] Update iso_fuse for uid/gid
* [ ] Update iso_fuse for mode
* [ ] Update iso_fuse for timestamps
* [ ] API for specifying/overriding filename encoding (defaults to UTF-8, but the SUSP/RR specs don't define any requirements here)

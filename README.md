cdfs
====
ISO 9660 / ECMA-119 filesystem implementation written in Rust.  It's still very much a work-in-progress.

Usage
-----
See the examples and the documentation for more information.

Extensions
----------
| System Use Sharing Protocol  |     |
| ---------------------------- | --- |
| `CE` – continuation area     | yes |
| `PD` – padding field         | no  |
| `SP` – SUSP start            | *not enforced* |
| `ST` – SUSP end              | *ignored*      |
| `ER` – extensions reference  | yes |
| `ES` – extensions selector   | no  |

| Rock Ridge Interchange       |     |
| ---------------------------- |---- |
| `PX` – POSIX attributes      | yes |
| `PN` – POSIX inodes          | yes |
| `SL` – symbolic links        | yes |
| `NM` – long file names       | yes |
| `CL` – child links           | yes |
| `PL` – parent links          | N/A |
| `RE` – relocated directories | yes |
| `TF` – file timestamps       | yes |
| `SF` – sparse files          | no  |

References
----------
* [ECMA-119 standard](https://www.ecma-international.org/publications/standards/Ecma-119.htm)
* [ISO 9660 on the OSDev wiki](https://wiki.osdev.org/ISO_9660)
* [Joliet](http://web.archive.org/web/20230604235448/http://littlesvr.ca/isomaster/resources/JolietSpecification.html)
* [Linux kernel isofs module](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/fs/isofs)
* [Rock Ridge v1.12](http://web.archive.org/web/20230107022048/https://aminet.net/package/docs/misc/RRIP)
* [Rock Ridge v1.10](http://web.archive.org/web/20220924230749/https://ia800605.us.archive.org/16/items/enf_pobox_Rrip/rrip.pdf)
* [Rock Ridge v1.09](http://web.archive.org/web/20230826015627/http://ftpmirror.your.org/pub/misc/bitsavers/projects/cdrom/Rock_Ridge_CD_Proposal_199108.pdf)
* [Wikipedia article on ISO 9660](https://en.wikipedia.org/wiki/ISO_9660)

Licensing
---------

This project is available under the Apache 2.0 or MIT license at your choosing.

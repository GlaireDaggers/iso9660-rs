// SPDX-License-Identifier: (MIT OR Apache-2.0)

use iso9660::{DirectoryEntry, ISO9660, ISO9660Reader};
use std::fs::File;

const JOLIET_IMAGE : &'static str = "images/joliet.iso";

fn filename_collector<I, T> (iter: I) -> Vec<String>
where
    I: Iterator< Item = iso9660::Result<DirectoryEntry<T>>>,
    T: ISO9660Reader,
{
    iter.map(|ent| match ent.unwrap() {
        DirectoryEntry::Directory(dirent) => {
            dirent.identifier
        },
        DirectoryEntry::File(fileent) => {
            fileent.identifier
        }
    })
    .collect()
}

#[test]
fn joliet_long_filenames_exist() {
    let fs = ISO9660::new(File::open(JOLIET_IMAGE).expect("couldn't open file")).expect("file is not an ISO image");

    let contents = filename_collector(fs.root().contents());
    assert_eq!(
        contents,
        &[
            ".",
            "..",
            "Read Me First, Please.txt",
            "readme.txt",
        ]
    );
}

#[test]
fn joliet_short_filenames_exist() {
    let fs = ISO9660::new(File::open(JOLIET_IMAGE).expect("couldn't open file")).expect("file is not an ISO image");

    let contents = filename_collector(fs.root_at(0).unwrap().contents());
    assert_eq!(
        contents,
        &[
            ".",
            "..",
            "README.TXT",
            "READ_ME_.TXT",
        ]
    );
}

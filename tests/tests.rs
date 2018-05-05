extern crate iso9660;
extern crate md5;

use std::io::Read;
use iso9660::{ISO9660, DirectoryEntry};

#[test]
fn test_dir() {
    let fs = ISO9660::new("test.iso").unwrap();

    let mut iter = fs.root.contents();
    assert_eq!(iter.next().unwrap().unwrap().identifier(), "."); 
    assert_eq!(iter.next().unwrap().unwrap().identifier(), ".."); 
    assert_eq!(iter.next().unwrap().unwrap().identifier(), "A"); 
    assert_eq!(iter.next().unwrap().unwrap().identifier(), "GPL_3_0.TXT"); 
    assert!(iter.next().is_none()); 
}

#[test]
fn test_large_file() {
    let fs = ISO9660::new("test.iso").unwrap();

    let mut file = match fs.open("gpl_3_0.txt").unwrap().unwrap() {
        DirectoryEntry::File(file) => file,
        _ => panic!("Not a file")
    };

    let mut text = String::new();
    file.read_to_string(&mut text).unwrap();
    let hash = md5::compute(text);
    assert_eq!(format!("{:x}", hash), "1ebbd3e34237af26da5dc08a4e440464");
}

#[test]
fn test_extra_slashes() {
    let fs = ISO9660::new("test.iso").unwrap();

    assert!(fs.open("///a/b/c/1").unwrap().is_some());
    assert!(fs.open("a/b/c/1///").unwrap().is_some());
    assert!(fs.open("a/b//c/1").unwrap().is_some());
    assert!(fs.open("/a/b//c////1/").unwrap().is_some());
}

#[test]
fn test_large_dir() {
    let fs = ISO9660::new("test.iso").unwrap();

    let dir = match fs.open("a/b/c").unwrap().unwrap() {
        DirectoryEntry::Directory(dir) => dir,
        _ => panic!("Not a directory")
    };

    // 200 files, plus '.' and '..'
    assert_eq!(dir.contents().map(Result::unwrap).count(), 202);
    assert_eq!(dir.block_count(), 4);
}
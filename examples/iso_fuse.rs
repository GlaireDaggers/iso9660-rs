// SPDX-License-Identifier: (MIT OR Apache-2.0)

use std::{
    collections::HashMap,
    convert::TryFrom,
    ffi::OsStr,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::Duration,
};

use clap::Parser;
use fuser::{ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, Request};
use libc::{EISDIR, ENOENT, ENOTDIR, EPROTO};

use iso9660::{DirectoryEntry, ISODirectory, ISOFileReader, ISO9660};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    iso_path: PathBuf,
    mountpoint: PathBuf,
}

fn entry_to_filetype(entry: &DirectoryEntry<File>) -> fuser::FileType {
    match entry {
        DirectoryEntry::Directory(_) => fuser::FileType::Directory,
        DirectoryEntry::File(_) => fuser::FileType::RegularFile,
    }
}

fn get_fileattr(ino: u64, entry: &DirectoryEntry<File>) -> fuser::FileAttr {
    let blocks = (entry.header().extent_length + 2048 - 1) / 2048; // ceil(len / 2048
    let time = entry.header().time.into();

    // If the goal is to allow a non-privileged user to view things, let's
    // default to our own UID/GID.  A more useful implementation would
    // allow the end user to override this and never use the Rockridge
    // POSIX info so that they can inspect the whole filesystem.
    let uid = unsafe { libc::geteuid() };
    let gid = unsafe { libc::getegid() };

    // Okay. File permissions are *octal*, not decimal.  Unlike some (many?) other languages Rust
    // will assume a numeric literal is in base-10 even if it starts with a leading 0.  For Rust
    // to assume an octal literal it must start with `0o`.
    //
    // Directories should have their execute bit set so that we can list their contents.
    // Files shouldn't to be extra paranoid.
    let perm = match entry {
        DirectoryEntry::Directory(_) => 0o0555,
        DirectoryEntry::File(_) => 0o0444,
    };

    fuser::FileAttr {
        ino,
        size: u64::from(entry.header().extent_length),
        blocks: u64::from(blocks),
        atime: time,
        mtime: time,
        ctime: time,
        crtime: time,
        kind: entry_to_filetype(&entry),
        perm,
        nlink: 1,
        uid,
        gid,
        rdev: 0,
        flags: 0,
        blksize: 2048,
    }
}

struct ISOFuse {
    _iso9660: ISO9660<File>,
    inodes: HashMap<u64, DirectoryEntry<File>>,
    inode_number: u64,
    directory_number: u64,
    file_number: u64,
    open_directories: HashMap<u64, ISODirectory<File>>,
    open_files: HashMap<u64, ISOFileReader<File>>,
}

impl ISOFuse {
    fn new(path: String) -> Self {
        let file = File::open(path).unwrap();
        let iso9660 = ISO9660::new(file).unwrap();
        let mut inodes = HashMap::new();
        inodes.insert(
            fuser::FUSE_ROOT_ID,
            DirectoryEntry::Directory(iso9660.root.clone()),
        );
        Self {
            _iso9660: iso9660,
            inodes,
            inode_number: fuser::FUSE_ROOT_ID + 1,
            file_number: 0,
            directory_number: 0,
            open_files: HashMap::new(),
            open_directories: HashMap::new(),
        }
    }
}

impl fuser::Filesystem for ISOFuse {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let entry = self.inodes.get(&parent).unwrap();
        if let DirectoryEntry::Directory(directory) = entry {
            match directory.find(name.to_str().unwrap()) {
                Ok(Some(entry)) => {
                    let fileattr = get_fileattr(self.inode_number, &entry);
                    self.inodes.insert(self.inode_number, entry);
                    self.inode_number += 1;

                    reply.entry(&Duration::from_secs(0), &fileattr, 0);
                }
                Ok(None) => reply.error(ENOENT),
                Err(_) => reply.error(EPROTO),
            }
        } else {
            reply.error(ENOTDIR);
        }
    }

    fn forget(&mut self, _req: &Request, ino: u64, _nlookup: u64) {
        self.inodes.remove(&ino);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let entry = self.inodes.get(&ino).unwrap();
        let fileattr = get_fileattr(ino, entry);
        reply.attr(&Duration::from_secs(0), &fileattr);
    }

    fn open(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let entry = self.inodes.get(&ino).unwrap();
        if let DirectoryEntry::File(file) = entry {
            self.open_files.insert(self.file_number, file.read());
            reply.opened(self.file_number, 0);
            self.file_number += 1;
        } else {
            reply.error(EISDIR)
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        _ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let offset = u64::try_from(offset).expect("SeekFrom::Start must be positive int");
        let size = usize::try_from(size).expect("size didn't fit into usize?");

        let file = self.open_files.get_mut(&fh).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        let mut buf = vec![0; size];
        let count = file.read(&mut buf).unwrap();
        reply.data(&buf[..count]);
    }

    fn release(
        &mut self,
        _req: &Request,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        self.open_files.remove(&fh);
        reply.ok();
    }

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let entry = self.inodes.get(&ino).unwrap();
        if let DirectoryEntry::Directory(directory) = entry {
            self.open_directories
                .insert(self.directory_number, directory.clone());
            reply.opened(self.directory_number, 0);
            self.directory_number += 1;
        } else {
            reply.error(ENOTDIR)
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        _ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let dir = self.open_directories.get(&fh).unwrap();

        if offset == -1 {
            reply.ok();
            return;
        }

        let mut block = [0; 2048];
        let mut block_num = None;
        let mut offset = offset as u64;

        loop {
            let (entry, next_offset) = dir
                .read_entry_at(&mut block, &mut block_num, offset)
                .unwrap();

            let kind = entry_to_filetype(&entry);
            if reply.add(
                self.inode_number,
                next_offset.map(|x| x as i64).unwrap_or(-1),
                kind,
                entry.identifier(),
            ) {
                break;
            }

            self.inodes.insert(self.inode_number, entry);
            self.inode_number += 1;

            if let Some(next_offset) = next_offset {
                offset = next_offset;
            } else {
                break;
            }
        }

        reply.ok();
    }

    fn releasedir(&mut self, _req: &Request, _ino: u64, fh: u64, _flags: i32, reply: ReplyEmpty) {
        self.open_directories.remove(&fh);
        reply.ok();
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Ideally ISOFuse::new should take an AsRef<Path>
    let iso_path = args.iso_path.to_string_lossy().to_string();

    fuser::mount2(ISOFuse::new(iso_path), &args.mountpoint, &[])?;

    Ok(())
}

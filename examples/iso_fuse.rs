// SPDX-License-Identifier: (MIT OR Apache-2.0)

#[allow(unused)]
use log::{debug, error, info, trace, warn};

use std::{
    collections::HashMap,
    convert::TryFrom,
    ffi::OsStr,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use clap::Parser;
use fuser::{
    MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen,
    ReplyXattr, Request,
};
use libc::{EINVAL, EIO, EISDIR, ENFILE, ENOENT, ENOTDIR, ENOTSUP};

use cdfs::{
    BlockBuffer, BlockBufferCtor, DirectoryEntry, ExtraAttributes, ISODirectory, ISOFileReader,
    BLOCK_SIZE, ISO9660,
};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    iso_path: PathBuf,
    mountpoint: PathBuf,
}

fn entry_to_filetype(entry: &DirectoryEntry<File>) -> fuser::FileType {
    match entry {
        DirectoryEntry::Directory(_) => fuser::FileType::Directory,
        DirectoryEntry::File(_) => fuser::FileType::RegularFile,
        DirectoryEntry::Symlink(_) => fuser::FileType::Symlink,
    }
}

fn get_fileattr(ino: u64, entry: &DirectoryEntry<File>) -> fuser::FileAttr {
    let blksize = u32::from(BLOCK_SIZE);
    let blocks = (entry.header().extent_length + blksize - 1) / blksize; // ceil(len / blksize)
    let blocks = u64::from(blocks);
    let size = u64::from(entry.header().extent_length);

    let atime = entry.access_time().into();
    let ctime = entry.attribute_change_time().into();
    let crtime = entry.create_time().into();
    let mtime = entry.modify_time().into();

    // If the goal is to allow a non-privileged user to view things, let's default to our own
    // UID/GID.  A more useful implementation would allow the end user to override this and never
    // use the Rock Ridge POSIX info so that they can inspect the whole filesystem.
    let uid = entry.owner().unwrap_or_else(|| unsafe { libc::geteuid() });
    let gid = entry.group().unwrap_or_else(|| unsafe { libc::getegid() });

    // Okay. File permissions are *octal*, not decimal.  Unlike some (many?) other languages Rust
    // will assume a numeric literal is in base-10 even if it starts with a leading 0.  For Rust
    // to assume an octal literal it must start with `0o`.
    let perm = match entry.mode() {
        Some(mode) => u16::from(mode),
        None => match entry {
            DirectoryEntry::Directory(_) => 0o0555,
            DirectoryEntry::File(_) => 0o0444,
            DirectoryEntry::Symlink(_) => 0o0444,
        },
    };

    fuser::FileAttr {
        ino,
        size,
        blocks,
        atime,
        mtime,
        ctime,
        crtime,
        kind: entry_to_filetype(entry),
        perm,
        nlink: 1,
        uid,
        gid,
        rdev: 0,
        flags: 0,
        blksize,
    }
}

struct ISOFuse {
    _iso9660: ISO9660<File>,
    inodes: HashMap<u64, DirectoryEntry<File>>,
    inode_number: u64,
    inode_autogenerated: bool,
    directory_number: u64,
    file_number: u64,
    open_directories: HashMap<u64, ISODirectory<File>>,
    open_files: HashMap<u64, ISOFileReader<File>>,
}

impl ISOFuse {
    fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let file = File::open(path.as_ref()).context("Could not open ISO image")?;
        let iso9660 = ISO9660::new(file).context("Could not parse ISO image")?;

        let mut inodes = HashMap::new();
        let root = iso9660.root();
        let slashdot = root.contents().next().unwrap().unwrap();

        // We could just check which extensions are registered, but inodes didn't
        // come in until v1.12, and most implementations seem to only generate
        // v1.09 ER records.
        let inode_autogenerated = !(iso9660.is_rr() && slashdot.inode().is_some());

        if !inode_autogenerated {
            info!("Found POSIX.1 extensions with usable inodes.");
        }

        inodes.insert(fuser::FUSE_ROOT_ID, DirectoryEntry::Directory(root.clone()));

        Ok(Self {
            _iso9660: iso9660,
            inodes,
            inode_number: fuser::FUSE_ROOT_ID + 1,
            inode_autogenerated,
            file_number: 0,
            directory_number: 0,
            open_files: HashMap::new(),
            open_directories: HashMap::new(),
        })
    }
}

impl fuser::Filesystem for ISOFuse {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let parent_entry = match self.inodes.get(&parent) {
            Some(parent_entry) => parent_entry,
            None => return reply.error(EINVAL),
        };

        if let DirectoryEntry::Directory(parent_directory) = parent_entry {
            if self.inode_number == u64::MAX {
                return reply.error(ENFILE);
            }

            if let Ok(Some(current_entry)) = parent_directory.find(name.to_str().unwrap()) {
                let inode_number = match self.inode_autogenerated {
                    true => self.inode_number,
                    false => u64::from(current_entry.inode().expect("missing inode?!")),
                };

                let fileattr = get_fileattr(inode_number, &current_entry);
                self.inodes.insert(inode_number, current_entry);

                if self.inode_autogenerated {
                    self.inode_number += 1;
                }

                reply.entry(&Duration::from_secs(0), &fileattr, 0);
            } else {
                reply.error(ENOENT);
            }
        } else {
            reply.error(ENOTDIR);
        }
    }

    fn forget(&mut self, _req: &Request, ino: u64, _nlookup: u64) {
        if self.inodes.remove(&ino).is_none() {
            warn!("Attempting to forget non-existant inode: {ino:?}");
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let entry = match self.inodes.get(&ino) {
            Some(entry) => entry,
            None => return reply.error(EINVAL),
        };

        let fileattr = get_fileattr(ino, entry);
        reply.attr(&Duration::from_secs(0), &fileattr);
    }

    fn readlink(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyData) {
        let entry = match self.inodes.get(&ino) {
            Some(entry) => entry,
            None => return reply.error(EINVAL),
        };

        if let DirectoryEntry::Symlink(symlink) = entry {
            match symlink.target() {
                Some(target) => reply.data(target.as_bytes()),
                None => reply.error(EINVAL),
            }
        } else {
            reply.error(EINVAL)
        }
    }

    fn open(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let entry = match self.inodes.get(&ino) {
            Some(entry) => entry,
            None => return reply.error(EINVAL),
        };

        if let DirectoryEntry::File(file) = entry {
            if self.file_number == u64::MAX {
                return reply.error(ENFILE);
            }

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
        // Two things of note here:
        // * using u64::try_from instead of casting with as ensures there's no silent over/underflow
        // * returning the result of reply.error works because it returns "void" and so do we
        let offset = match u64::try_from(offset) {
            Ok(offset) => offset,
            Err(_) => return reply.error(EINVAL),
        };

        let size = match usize::try_from(size) {
            Ok(size) => size,
            Err(_) => return reply.error(EINVAL),
        };

        let file = match self.open_files.get_mut(&fh) {
            Some(file) => file,
            None => return reply.error(EINVAL),
        };

        if file.seek(SeekFrom::Start(offset)).is_err() {
            return reply.error(EIO);
        }

        let mut buf = vec![0; size];
        let count = match file.read(&mut buf) {
            Ok(count) => count,
            Err(_) => return reply.error(EIO),
        };

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
        match self.open_files.remove(&fh) {
            Some(_) => reply.ok(),
            None => reply.error(EINVAL),
        }
    }

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        let entry = match self.inodes.get(&ino) {
            Some(entry) => entry,
            None => return reply.error(EINVAL),
        };

        if let DirectoryEntry::Directory(directory) = entry {
            if self.directory_number == u64::MAX {
                return reply.error(ENFILE);
            }

            self.open_directories
                .insert(self.directory_number, directory.clone());
            reply.opened(self.directory_number, 0);
            self.directory_number += 1;
        } else {
            reply.error(ENOTDIR)
        }
    }

    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _size: u32,
        reply: ReplyXattr,
    ) {
        reply.error(ENOTSUP)
    }

    fn readdir(
        &mut self,
        _req: &Request,
        _ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let dir = match self.open_directories.get(&fh) {
            Some(dir) => dir,
            None => return reply.error(EINVAL),
        };

        if offset == -1 {
            return reply.ok();
        }

        let mut block = BlockBuffer::new();
        let mut block_num = None;
        let mut offset = match u64::try_from(offset) {
            Ok(offset) => offset,
            Err(_) => return reply.error(EINVAL),
        };

        loop {
            if self.inode_number == u64::MAX {
                return reply.error(ENFILE);
            }

            let entry = dir.read_entry_at(&mut block, &mut block_num, offset);
            let (entry, next_offset) = match entry {
                Ok((dirent, next_offset)) => {
                    // While we have access to the relocated directories both at their original
                    // location and their new location, most operating systems don't allow
                    // directories to be hardlinked.  Thus if the relocated flag is set, hide it
                    // from FUSE.
                    if dirent.relocated() {
                        offset = match next_offset {
                            Some(offset) => offset,
                            None => break,
                        };
                        continue;
                    } else {
                        (dirent, next_offset)
                    }
                }
                Err(_) => return reply.error(EINVAL),
            };

            let fuse_offset = match next_offset.map(i64::try_from).transpose() {
                Ok(next_offset) => next_offset.unwrap_or(-1),
                Err(_) => return reply.error(EINVAL),
            };

            let kind = entry_to_filetype(&entry);

            let inode_number = match self.inode_autogenerated {
                true => self.inode_number,
                false => {
                    if entry.inode().is_none() {
                        warn!("missing inode for {:?}", entry.identifier());
                    }
                    u64::from(entry.inode().unwrap())
                }
            };

            if reply.add(inode_number, fuse_offset, kind, entry.identifier()) {
                error!("Error adding {inode_number:?}, {:?}", entry.identifier());
                break;
            }

            if entry.identifier() != "." && entry.identifier() != ".." {
                self.inodes.insert(inode_number, entry);
            }

            if self.inode_autogenerated {
                self.inode_number += 1;
            }

            if let Some(next_offset) = next_offset {
                offset = next_offset;
            } else {
                break;
            }
        }

        reply.ok();
    }

    fn releasedir(&mut self, _req: &Request, _ino: u64, fh: u64, _flags: i32, reply: ReplyEmpty) {
        match self.open_directories.remove(&fh) {
            Some(_) => reply.ok(),
            None => reply.error(EINVAL),
        }
    }
}

fn main() -> anyhow::Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("fuser", log::LevelFilter::Info)
        .env()
        .init()?;

    let args = Args::parse();

    info!("NOTE: The filesystem must be manually unmounted after exit");

    fuser::mount2(
        ISOFuse::new(args.iso_path)?,
        &args.mountpoint,
        &[MountOption::RO],
    )?;

    Ok(())
}

// SPDX-License-Identifier: (MIT OR Apache-2.0)

extern crate iso9660;

use std::{fs::File, path::PathBuf};

use anyhow::{anyhow, bail};
use clap::Parser;

use iso9660::{DirectoryEntry, ISO9660Reader, ISODirectory, ISO9660};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    iso_path: PathBuf,
    dir_path: Option<PathBuf>,
}

const INDENT: &'static str = "  ";

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let file = File::open(args.iso_path)?;
    let fs = ISO9660::new(file)?;

    match args.dir_path {
        Some(dir_path) => {
            let dir_path = dir_path
                .to_str()
                .ok_or_else(|| anyhow!("{dir_path:?} could not be converted to a UTF-8 string"))?;

            match fs.open(&dir_path)? {
                Some(DirectoryEntry::Directory(dir)) => {
                    print_tree(&dir, 0);
                }
                Some(DirectoryEntry::File(_)) => {
                    bail!("'{dir_path}' is not a directory");
                }
                None => {
                    bail!("'{dir_path}' does not exist");
                }
            }
        }
        None => print_tree(&fs.root, 0),
    }

    Ok(())
}

fn print_tree<T: ISO9660Reader>(dir: &ISODirectory<T>, level: usize) {
    for entry in dir.contents() {
        match entry.unwrap() {
            DirectoryEntry::Directory(dir) => {
                if dir.identifier == "." || dir.identifier == ".." {
                    continue;
                }

                print!("{}", INDENT.repeat(level));
                println!("- {}/", dir.identifier);
                print_tree(&dir, level + 1);
            }
            DirectoryEntry::File(file) => {
                print!("{}", INDENT.repeat(level));
                println!("- {}", file.identifier);
            }
        }
    }
}

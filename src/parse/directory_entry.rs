// SPDX-License-Identifier: (MIT OR Apache-2.0)

use std::str;

use nom::{
    branch::alt,
    bytes::complete::tag,
    combinator::{map_parser, value},
    multi::length_data,
    number::complete::le_u8,
    IResult,
};
use time::OffsetDateTime;

use super::{
    both_endian::{both_endian16, both_endian32},
    date_time::date_time,
    decode_string, CharacterEncoding,
};
use crate::Result;

bitflags! {
    #[derive(Clone, Debug)]
    pub struct FileFlags: u8 {
        const EXISTANCE = 1 << 0;
        const DIRECTORY = 1 << 1;
        const ASSOCIATEDFILE = 1 << 2;
        const RECORD = 1 << 3;
        const PROTECTION = 1 << 4;
        // Bits 5 and 6 are reserved; should be zero
        const MULTIEXTENT = 1 << 7;
    }
}

#[derive(Clone, Debug)]
pub struct DirectoryEntryHeader {
    pub length: u8,
    pub extended_attribute_record_length: u8,
    pub extent_loc: u32,
    pub extent_length: u32,
    pub time: OffsetDateTime,
    pub file_flags: FileFlags,
    pub file_unit_size: u8,
    pub interleave_gap_size: u8,
    pub volume_sequence_number: u16,
    pub character_encoding: CharacterEncoding,
}

impl DirectoryEntryHeader {
    pub fn parse(
        input: &[u8],
        character_encoding: CharacterEncoding,
    ) -> Result<(DirectoryEntryHeader, String)> {
        Ok(directory_entry(input, character_encoding)?.1)
    }
}

pub fn directory_entry<'a>(
    i: &'a [u8],
    character_encoding: CharacterEncoding,
) -> IResult<&[u8], (DirectoryEntryHeader, String)> {
    let (i, length) = le_u8(i)?;
    let (i, extended_attribute_record_length) = le_u8(i)?;
    let (i, extent_loc) = both_endian32(i)?;
    let (i, extent_length) = both_endian32(i)?;
    let (i, time) = date_time(i)?;
    let (i, file_flags) = le_u8(i)?;
    let (i, file_unit_size) = le_u8(i)?;
    let (i, interleave_gap_size) = le_u8(i)?;
    let (i, volume_sequence_number) = both_endian16(i)?;

    let (i, identifier) = map_parser(
        length_data(le_u8),
        |bytes: &'a [u8]| -> IResult<&'a [u8], String> {
            if bytes.len() == 1 {
                alt((
                    value(String::from("\u{0}"), tag(&[0])),
                    value(String::from("\u{1}"), tag(&[1])),
                    decode_string(character_encoding),
                ))(bytes)
            } else {
                decode_string(character_encoding)(bytes)
            }
        },
    )(i)?;

    // After the file identifier, ISO 9660 allows addition space for
    // system use. Ignore that for now.

    Ok((
        i,
        (
            DirectoryEntryHeader {
                length,
                extended_attribute_record_length,
                extent_loc,
                extent_length,
                time,
                file_flags: FileFlags::from_bits_truncate(file_flags),
                file_unit_size,
                interleave_gap_size,
                volume_sequence_number,
                character_encoding,
            },
            identifier,
        ),
    ))
}

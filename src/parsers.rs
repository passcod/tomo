use deku::{ctx::Endian, prelude::*};
use std::mem::size_of;

// *_SIZE constants measure the packed (deku) size, not the layout in memory (rust) size.

pub const MAGIC: [u8; 7] = *b"\0T\0M\0v\x01";

#[derive(Clone, Debug, Default, DekuRead, DekuWrite)]
#[deku(magic = b"\0T\0M\0v\x01", endian = "little")]
pub struct ContainerHeader {
	pub mode: Mode,
	pub index_bytes: u64,
	pub entries_bytes: u64,
}

pub const CONTAINER_HEADER_SIZE: usize =
	MAGIC.len() + size_of::<Mode>() + size_of::<u64>() + size_of::<u64>();
static_assertions::const_assert_eq!(CONTAINER_HEADER_SIZE, 24);

// format notes:
//
// - index is an array of 24-byte structs (each called an "indic"). indics contain a type byte, an
// optional (zero = null) path index value, an optional (zero = null) attributes index value, and
// an offset + length for the associated data entry. offsets are from start of data section.
// - the path index value refers to a (1-indexed) item in the special Paths data entry. that item
// contains the path of the archived object. if the value is zero, the indic refers to the tomo
// archive itself, usually used for the special types (0xF0 and above).
// - the attributes index value is as with the path, but in the special Attributes data entry.
// - each data entry is preceded by a header describing the compression of the data.
// - each data entry is individually compressed/encoded.
// - there can be several layers of encoding.
// - each data entry can use a different encoding.
// - two index entries can point to the same data entry. this can be either that the files are
// duplicated, or that the files are contained in a nested tomo container (or more, with catting).
// - typical tomo metadata read sequence:
//   1. match magic, read header for mode and lengths
//   2. start reading the index
//   3. find the Paths (0xF0) and if present the Checksums (0xF1) and Signatures (0xF2) indics
//   4. decompress/decode these entries
//   5. parse the paths data entry
//   6. check the checksums and the signatures if provided and/or required
//   7. if there's another tomo archive catted (detected at step 2), read its index too, etc
// - this is pretty well parallelised / adapted to async. reading the special entries can happen as
// soon as you've found them in the index, while continuing the index read, for example.
// - archive mode is about archive concatenating: default is Stacked: given a path that exists in
// two archives, the latter archive it appears in "wins."
// - archives can also be given sequentially to tomo for their mode to apply, there's no
// requirement that they be catted.
// - paths are stored in a platform-independent format, broken in their components ("segments").
// it's possible to have absolute paths, URLs, drive-rooted paths, etc. Path segments are arbitrary
// byte vecs, which may include null bytes, so tomo can be used to archive e.g. arbitrary KV data,
// not just files.
// - there's a limit of 16 million paths and 16 million attributes per tomo container, but you can
// exceed that limit in a single file by catting.
// - zstd dictionary mode is natively supported and the default on cli.

#[derive(Clone, Copy, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(type = "u8", ctx = "_: Endian")]
#[repr(u8)]
pub enum Mode {
	#[deku(id = "0x01")]
	Stacked = 1,
}

impl Default for Mode {
	fn default() -> Self {
		Self::Stacked
	}
}

#[derive(Clone, Copy, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(type = "u8", ctx = "_: Endian")]
pub enum IndicKind {
	#[deku(id = "0x01")]
	File,
	#[deku(id = "0x02")]
	Dir,

	#[deku(id = "0x10")]
	Attributes,

	#[deku(id = "0xF0")]
	Paths,
	#[deku(id = "0xF1")]
	Checksums,
	#[deku(id = "0xF2")]
	Signatures,
}

#[derive(Clone, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(type = "u8", ctx = "_: Endian")]
pub enum PathSeg {
	#[deku(id = "0x01")]
	Segment(#[deku(until = "|v| *v == 0")] Vec<u8>),

	#[deku(id = "0x10")]
	Root,
}

#[derive(Clone, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(ctx = "endian: Endian")]
pub struct Path {
	#[deku(update = "self.segments.len()")]
	segcount: u32,
	#[deku(count = "segcount", ctx = "endian")]
	segments: Vec<PathSeg>,
}

#[derive(Clone, Copy, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(ctx = "_: Endian")]
pub struct Lookup {
	index: u32,
	offset: u64,
}
pub const LOOKUP_SIZE: usize = size_of::<u32>() + size_of::<u64>();
static_assertions::const_assert_eq!(LOOKUP_SIZE, 12);

fn write_lookup<T: DekuWrite<Endian>>(
	list: &Vec<T>,
	output: &mut BitVec<Msb0, u8>,
	ctx: Endian,
) -> Result<(), DekuError> {
	use std::io::{Cursor, Write};

	let path_count = list.len();
	(path_count as u32).write(output, ctx)?;

	let lookup_offset = output.len();
	let lookup_length = path_count * LOOKUP_SIZE;
	let lookup = vec![0; lookup_length];
	lookup.write(output, ())?;
	let mut lookup = Cursor::new(lookup);

	for (index, item) in list.iter().enumerate() {
		let index = (index as u32) + 1;
		let offset = output.len() as u64;
		item.write(output, ctx)?;

		// unwrap: infaillible
		lookup.write(&index.to_le_bytes()).unwrap();
		lookup.write(&offset.to_le_bytes()).unwrap();
	}

	let mut lookup: BitVec<Msb0, u8> = BitVec::try_from_vec(lookup.into_inner()).unwrap();
	output[lookup_offset..(lookup_offset + lookup_length * 8)].swap_with_bitslice(&mut lookup);
	Ok(())
}

#[derive(Clone, Debug, DekuRead, Eq, PartialEq, Ord, PartialOrd)]
#[deku(endian = "little")]
pub struct PathsEntry {
	#[deku(bytes = 4)]
	path_count: usize,
	#[deku(
		count = "*path_count * LOOKUP_SIZE",
		map = "|_: Vec<u8>| -> Result<(), DekuError> { Ok(()) }"
	)]
	_lookup: (), // parsed but discarded (only useful when doing partial parses)
	#[deku(count = "path_count")]
	paths: Vec<Path>,
}

impl DekuWrite<Endian> for PathsEntry {
	fn write(&self, output: &mut BitVec<Msb0, u8>, ctx: Endian) -> Result<(), DekuError> {
		write_lookup(&self.paths, output, ctx)
	}
}

#[derive(Clone, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(ctx = "_: Endian")]
pub struct Attributes {
	mode: u16,
}

#[derive(Clone, Debug, DekuRead, Eq, PartialEq, Ord, PartialOrd)]
#[deku(endian = "little")]
pub struct AttributesEntry {
	#[deku(bytes = 4)]
	attr_count: usize,
	#[deku(
		count = "*attr_count * LOOKUP_SIZE",
		map = "|_: Vec<u8>| -> Result<(), DekuError> { Ok(()) }"
	)]
	_lookup: (), // parsed but discarded (only useful when doing partial parses)
	#[deku(count = "attr_count")]
	attrs: Vec<Attributes>,
}

impl DekuWrite<Endian> for AttributesEntry {
	fn write(&self, output: &mut BitVec<Msb0, u8>, ctx: Endian) -> Result<(), DekuError> {
		write_lookup(&self.attrs, output, ctx)
	}
}

#[derive(Clone, Copy, Debug, DekuRead, DekuWrite)]
#[deku(endian = "little")]
pub struct Indic {
	pub kind: IndicKind,
	#[deku(bytes = 3)]
	pub path: u32,
	#[deku(bytes = 3)]
	pub attrs: u32,
	_reserved: u8,
	pub offset: u64,
	pub length: u64,
}

pub const INDIC_SIZE: u64 = (size_of::<IndicKind>() +
    3 + // "u24"
    3 + // "u24"
    size_of::<u8>() +
    size_of::<u64>() +
    size_of::<u64>()) as u64;
static_assertions::const_assert_eq!(INDIC_SIZE, 24);

#[derive(Clone, Copy, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(type = "u8", ctx = "_: Endian")]
pub enum Encoding {
	#[deku(id = "0x00")]
	Raw,
	#[deku(id = "0x01")]
	Zstd,

	#[deku(id = "0xFE")]
	Custom,
	#[deku(id = "0xFF")]
	Tomo,
}

impl Default for Encoding {
	fn default() -> Self {
		Self::Raw
	}
}

#[derive(Clone, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(endian = "little")]
pub struct ZstdParams {
	/// Index of the indic that points to the zstd dictionary data file
	dictionary: u64,
}

#[derive(Clone, Debug, DekuRead, DekuWrite, Eq, PartialEq, Ord, PartialOrd)]
#[deku(type = "u8", endian = "little")]
pub enum CustomParams {
	#[deku(id = "0x01")]
	Program(#[deku(until = "|v: &u8| *v == 0")] Vec<u8>),
}

#[derive(Clone, Debug, Default, DekuRead, DekuWrite)]
#[deku(endian = "little")]
pub struct EntryHeader {
	#[deku(bits = 1)]
	has_params: u8,
	#[deku(bits = 1)]
	nested: u8,
	#[deku(bits = 6)]
	_reserved: u8,
	encoding: Encoding,
	#[deku(update = "self.params.len()", cond = "*has_params == 1", default = "0")]
	params_bytes: u16,
	#[deku(count = "params_bytes")]
	params: Vec<u8>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use deku::ctx::{Endian, Limit};
	use std::collections::HashMap;

	// below structures emulate reading/writing via deku for testing the parsers
	// but actual tomo code is all about reading only what needs to be, not all.
	#[derive(Clone, Debug, Default, DekuRead, DekuWrite)]
	#[deku(magic = b"\0T\0M\0v\x01", endian = "little")]
	pub struct TestContainer {
		mode: Mode,
		#[deku(
			update = "{ use crate::parsers::INDIC_SIZE; self.index.len() as u64 * INDIC_SIZE }"
		)]
		index_bytes: u64,
		#[deku(update = "self.entries.len()")]
		entries_bytes: u64,
		#[deku(count = "index_bytes / INDIC_SIZE")]
		index: Vec<TestIndic>,
		#[deku(bytes_read = "entries_bytes", ctx = "index")]
		entries: TestEntries,
	}

	#[derive(Clone, Copy, Debug, DekuRead, DekuWrite)]
	#[deku(ctx = "endian: Endian")]
	pub struct TestIndic {
		#[deku(ctx = "endian")]
		kind: IndicKind,
		#[deku(bytes = 3)]
		path: u32,
		#[deku(bytes = 3)]
		attrs: u32,
		_reserved: u8,
		offset: u64,
		length: u64,
	}

	#[derive(Clone, Debug)]
	pub struct TestEntry {
		indic: TestIndic,
		header: EntryHeader,
		data: Vec<u8>,
	}

	impl<T: Copy> DekuWrite<T> for TestEntry {
		fn write(&self, output: &mut BitVec<Msb0, u8>, _: T) -> Result<(), DekuError> {
			self.header.write(output, ())?;
			self.data.write(output, ())?;
			Ok(())
		}
	}

	#[derive(Clone, Debug, Default)]
	pub struct TestEntries {
		entries: Vec<TestEntry>,
		offsets: HashMap<u64, usize>,
	}

	impl TestEntries {
		pub fn len(&self) -> usize {
			self.entries.len()
		}

		pub fn from_offset(&self, offset: u64) -> Option<&TestEntry> {
			let index = *self.offsets.get(&offset)?;
			self.entries.get(index)
		}
	}

	impl
		DekuRead<(
			Limit<u8, for<'r> fn(&'r u8) -> bool>,
			(Endian, &Vec<TestIndic>),
		)> for TestEntries
	{
		fn read<'bs>(
			input: &'bs BitSlice<Msb0, u8>,
			ctx: (
				Limit<u8, for<'r> fn(&'r u8) -> bool>,
				(Endian, &Vec<TestIndic>),
			),
		) -> Result<(&'bs BitSlice<Msb0, u8>, Self), DekuError> {
			let (bits, index) = match ctx {
				(Limit::Bits(bits), (_, index)) => (*bits, index),
				_ => unreachable!("Entries should be read with bytes_read"),
			};

			let mut entries = Vec::with_capacity(index.len());
			let mut offsets = HashMap::new();

			// todo: record visited ranges and warn if there's extra

			for indic in index {
				let start = (indic.offset * 8) as usize;
				let length = (indic.length * 8) as usize;
				let end = start + length;

				let entry = &input[start..end];
				assert_eq!(entry.len(), length, "entry length remaining vs calculated");
				let (post_header, header) = EntryHeader::read(entry, ())?;
				let header_length = length - post_header.len();
				let data_length = length - header_length;
				let data_bits = &entry[header_length..];
				assert_eq!(
					data_bits.len(),
					data_length,
					"entry data length remaining vs calculated"
				);

				let (rest, data) = Vec::read(data_bits, ((data_length / 8).into(), ()))?;
				assert_eq!(rest.len(), 0, "remaining data after vec read");

				let ex = entries.len();
				entries.push(TestEntry {
					indic: *indic,
					header,
					data,
				});
				offsets.insert(indic.offset, ex);
			}

			Ok((&input[bits..], Self { entries, offsets }))
		}
	}

	impl<T: Copy> DekuWrite<T> for TestEntries {
		fn write(&self, output: &mut BitVec<Msb0, u8>, _: T) -> Result<(), DekuError> {
			for entry in &self.entries {
				entry.write(output, ())?;
			}

			Ok(())
		}
	}

	#[test]
	fn empty() {
		let mut data = Vec::new();
		data.extend(&MAGIC);
		data.push(Mode::Stacked as u8);
		data.extend(&0_u64.to_le_bytes());
		data.extend(&0_u64.to_le_bytes());
		dbg!(&data);

		let value = TestContainer::default();
		let data_out = value.to_bytes().unwrap();
		assert_eq!(data_out, data);
		dbg!(&data_out);

		let ((rest, _), value) = TestContainer::from_bytes((&data, 0)).unwrap();
		assert_eq!(rest.len(), 0);
		assert_eq!(value.mode, Mode::Stacked);
		assert_eq!(value.index.len(), 0);
		assert_eq!(value.entries.len(), 0);
	}

	#[test]
	fn empty_partial() {
		let mut data = Vec::new();
		data.extend(&MAGIC);
		data.push(Mode::Stacked as u8);
		data.extend(&0_u64.to_le_bytes());
		data.extend(&0_u64.to_le_bytes());

		let ((rest, _), value) = ContainerHeader::from_bytes((&data, 0)).unwrap();
		assert_eq!(rest.len(), 0);
		assert_eq!(value.mode, Mode::Stacked);
		assert_eq!(value.entries_bytes, 0);
		assert_eq!(value.index_bytes, 0);
	}

	#[test]
	fn catted() {
		let mut data = Vec::new();
		data.extend(&MAGIC);
		data.push(Mode::Stacked as u8);
		data.extend(&0_u64.to_le_bytes());
		data.extend(&0_u64.to_le_bytes());
		let datalen = data.len();
		let mut double = data.clone();
		double.extend(&data);

		let ((rest, _), value) = TestContainer::from_bytes((&double, 0)).unwrap();
		assert_eq!(rest.len(), datalen);
		assert_eq!(value.mode, Mode::Stacked);
		assert_eq!(value.entries.len(), 0);
		assert_eq!(value.index.len(), 0);

		let ((rest2, _), value) = TestContainer::from_bytes((&rest, 0)).unwrap();
		assert_eq!(rest2.len(), 0);
		assert_eq!(value.mode, Mode::Stacked);
		assert_eq!(value.entries.len(), 0);
		assert_eq!(value.index.len(), 0);
	}

	#[test]
	fn single_file_raw_lowlevel() {
		let mut ctnr = Vec::new();
		ctnr.extend(&MAGIC);
		ctnr.push(Mode::Stacked as u8);

		let pathsoffset = 0;
		let pathsdata = {
			let seg = b"\x01hello\0";

			let mut pathdata: Vec<u8> = Vec::new();
			pathdata.extend(&1_u32.to_le_bytes()); // count
			pathdata.extend(seg);

			let mut data = Vec::new();
			data.push(0b00_000000); // header: flags
			data.push(0x00); // header: encoding(raw)
			data.extend(&1_u32.to_le_bytes()); // count
			data.extend(&1_u32.to_le_bytes()); // index
			data.extend(&0_u64.to_le_bytes()); // offset
			data.extend(pathdata);
			data
		};

		let attrsoffset = pathsoffset + pathsdata.len();
		let attrsdata = {
			let mut attr: Vec<u8> = Vec::new();
			attr.extend(&0o644_u16.to_le_bytes()); // mode

			let mut data = Vec::new();
			data.push(0b00_000000); // header: flags
			data.push(0x00); // header: encoding(raw)
			data.extend(&1_u32.to_le_bytes()); // count
			data.extend(&1_u32.to_le_bytes()); // index
			data.extend(&0_u64.to_le_bytes()); // offset
			data.extend(attr);
			data
		};

		let fileoffset = attrsoffset + attrsdata.len();
		let filedata = {
			let file = b"Hello world!";
			let mut data = Vec::new();
			data.push(0b00_000000); // header: flags
			data.push(0x00); // header: encoding(raw)
			data.extend(file);
			data
		};

		let pathsindic = {
			let mut indic = Vec::new();
			indic.push(0xF0); // Paths
			indic.extend(&0_u32.to_le_bytes()[0..3]); // no path
			indic.extend(&0_u32.to_le_bytes()[0..3]); // no attr
			indic.push(0x00); // _reserved
			indic.extend(&pathsoffset.to_le_bytes()); // data offset
			indic.extend(&pathsdata.len().to_le_bytes()); // data length
			indic
		};

		let attrsindic = {
			let mut indic = Vec::new();
			indic.push(0x10); // Attributes
			indic.extend(&0_u32.to_le_bytes()[0..3]); // no path
			indic.extend(&0_u32.to_le_bytes()[0..3]); // no attr
			indic.push(0x00); // _reserved
			indic.extend(&attrsoffset.to_le_bytes()); // data offset
			indic.extend(&attrsdata.len().to_le_bytes()); // data length
			indic
		};

		let fileindic = {
			let mut indic = Vec::new();
			indic.push(0x01); // file
			indic.extend(&1_u32.to_le_bytes()[0..3]); // path
			indic.extend(&1_u32.to_le_bytes()[0..3]); // attr
			indic.push(0x00); // _reserved
			indic.extend(&fileoffset.to_le_bytes()); // data offset
			indic.extend(&filedata.len().to_le_bytes()); // data length
			indic
		};

		let index = {
			let mut index = Vec::new();
			index.extend(pathsindic);
			index.extend(attrsindic);
			index.extend(fileindic);
			index
		};

		let data = {
			let mut data = Vec::new();
			data.extend(pathsdata);
			data.extend(attrsdata);
			data.extend(filedata);
			data
		};

		ctnr.extend(&(3 * INDIC_SIZE).to_le_bytes());
		ctnr.extend(&(data.len() as u64).to_le_bytes());
		ctnr.extend(index);
		ctnr.extend(data);
		dbg!(&ctnr);

		let ((rest, _), value) = TestContainer::from_bytes((&ctnr, 0)).unwrap();
		dbg!(&value);
		assert_eq!(rest, &[]);
		assert_eq!(value.mode, Mode::Stacked);
		assert_eq!(value.entries.len(), 3);
		assert_eq!(value.index.len(), 3);

		let paths_indic = value
			.index
			.iter()
			.find(|indic| indic.kind == IndicKind::Paths)
			.unwrap();
		let paths_entry = value.entries.from_offset(paths_indic.offset).unwrap();
		assert_eq!(paths_entry.header.encoding, Encoding::Raw);
		let ((rest, _), pathsv) = PathsEntry::from_bytes((&paths_entry.data, 0)).unwrap();
		assert_eq!(rest.len(), 0, "remaining data on paths entry");
		assert_eq!(
			pathsv.paths,
			vec![Path {
				segcount: 1,
				segments: vec![PathSeg::Segment(b"hello\0".to_vec())],
			}]
		);

		let attrs_indic = value
			.index
			.iter()
			.find(|indic| indic.kind == IndicKind::Attributes)
			.unwrap();
		let attrs_entry = value.entries.from_offset(attrs_indic.offset).unwrap();
		assert_eq!(attrs_entry.header.encoding, Encoding::Raw);
		let ((rest, _), attrsv) = AttributesEntry::from_bytes((&attrs_entry.data, 0)).unwrap();
		assert_eq!(rest.len(), 0, "remaining data on attrs entry");
		assert_eq!(attrsv.attrs, vec![Attributes { mode: 0o644 }]);

		let file_indic = value
			.index
			.iter()
			.find(|indic| indic.kind == IndicKind::File)
			.unwrap();
		let file_entry = value.entries.from_offset(file_indic.offset).unwrap();
		assert_eq!(file_entry.header.encoding, Encoding::Raw);
		assert_eq!(file_entry.data, b"Hello world!".to_vec());
	}
}

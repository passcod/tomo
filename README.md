# Tomo

Yet another archive format.

_This is experimental, potentially unstable, possibly unmaintained, absolutely
not fuzzed nor audited in any way, and may contain bad ideas. Proceed with
caution._

Tomo has some interesting properties:

 - It's always possible to `cat` two archives together to add one to the other.
 - It's always possible to write and often possible to read a single file or subset of files efficiently.
 - It's always possible to read and write archives that are larger than memory.
 - It's always possible to parallelise reading and writing archives.

And some interesting features:

 - Archive paths are indexed (and extracting one file doesn't require reading
   the N files before it).
 - Archive contents can be compressed on a per-file basis (you can also
   compress multiple files together, see later).
 - The metadata can be compressed too.
 - Files can be deduplicated inside the archive, but the archive isn't a
   content-addressed store, so it's not automatic (but that means hashing
   collisions aren't necessarily a problem).
 - Both the archive and individual files support checksumming and signing as
   part of the format.
 - Compression with a dictionary is supported natively.
 - You can nest archives, such that you can compress a subset of the files
   together as a block, while still retaining indexing from the top level.
 - Each archive container defines its "`cat`ting" mode, so multi-container
   (catted) archives can emulate overlay filesystems (like docker) or have one
   container's contents have primacy over the rest, or go by modified date, or
   other strategies.
 - Paths are stored in a platform-independent format, with components split up,
   such that windows and unix paths syntax differences (mostly) don't matter.
 - Packing and unpacking are both done by reading the minimum required into
   memory, and reading from or writing to disk (or whatever byte source) as
   needed when needed, so memory requirements are kept low.
 - Both packing and unpacking are highly async processes, and can be
   parallelised as much as possible (but do not _require_ parallelism).
 - Yes, even with compression.

Tomo is designed:

 - To be `cat`ted directly onto an executable, such that a runtime and
   some application's source can be bundled together in one static file.
 - To support incremental construction.
 - To support being mounted as a read/write virtual filesystem.
 - To make use of multi-core and high-parallelism CPUs and I/O (SSDs).

Some "limitations" (so far):

 - Container size is limited to 18 exabytes
 - Each container is limited to 16 million files

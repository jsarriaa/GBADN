# GBADN format, version 2

A GBADN cartridge is a multi-record FASTA file: one layout record, then one record per chromosome. Each sequence encodes binary data, and together they rebuild the exact GBA ROM.

Cartridges are normally gzip-compressed (`.fasta.gz`). Decoders detect gzip from the file's contents and accept multi-member files, as written by `bgzip`.

## FASTA layout

```
>gbadn_layout format=GBADN2 title="GBADN DEMO" code=GDNA rom_bytes=1024 md5=fed2821399a5d92a58313b784b1a3808
CACTCAAGCACACATG...
>chrHeader rom_bytes=192
...
>chrData rom_bytes=832
...
```

Header lines are informational, and decoders must not rely on them. `code` is `NONE` when the ROM has no game code.

Encoders write uppercase `A`, `C`, `G` and `T`, with 70 bases per line and a trailing newline. Decoders also accept lowercase bases and CRLF line endings.

## Bases

Each byte becomes 4 bases, most significant bits first:

| Bits | Base |
|---|---|
| `00` | A |
| `01` | C |
| `10` | G |
| `11` | T |

For example, `G` = `0x47` = `01 00 01 11` = `CACT`.

## Layout record

The first record. Integers are little-endian.

| Offset | Size | Field | Value |
|---|---|---|---|
| 0 | 4 | Magic | ASCII `GBDN` |
| 4 | 1 | Version | `2` |
| 5 | 3 | Reserved | `0` |
| 8 | 4 | ROM size | Decoded size in bytes, at most 32 MiB |
| 12 | 4 | Segment count | Number of table entries |
| 16 | 16 | MD5 | Digest of the decoded ROM |
| 32 | 5 x n | Segment table | Kind (1 byte) and length (4 bytes) of each run |

The table lists the ROM's runs in ROM order. To rebuild the ROM, take each run's bytes from the next unread part of that kind's chromosome.

## Chromosome records

| Offset | Size | Field | Value |
|---|---|---|---|
| 0 | 1 | Kind | See below |
| 1 | 1 | Compression | `0` = none, `1` = zstd |
| 2 | 4 | Size | Decoded size in bytes |
| 6 | n | Payload | The chromosome's bytes, compressed as declared |

Encoders use zstd only when it makes the payload smaller. Kinds with no bytes have no record.

| Kind | Record | Holds |
|---|---|---|
| 0 | `chrHeader` | The 192-byte cartridge header |
| 1 | `chrCode` | ARM and Thumb code: the game's mechanics |
| 2 | `chrGraphics` | LZ77-compressed tile graphics |
| 3 | `chrAudio` | Sampled sound with a recognised header |
| 4 | `chrData` | Everything else: text, maps, music, uncompressed art |
| 5 | `chrEmpty` | Runs of `0x00` or `0xFF` |

### Accuracy of the kinds

A GBA ROM has no table of contents, so only the header and empty space are certain. The rest are best guesses by the encoder, working in 256-byte blocks:

- Graphics and audio are found by validating real structures: an LZ77 stream that decodes exactly to a tile-sized length, or a sound sample header followed by smoothly changing samples.
- Code is recognised by the density of calls, stack frames and returns.
- Short runs are absorbed by their neighbours.

A wrong guess never damages the game, because the layout rebuilds the same bytes whatever the labels say. The classifier can change without a format change.

## Decoding

A decoder must reject a file when any of these is true:

- It holds damaged gzip data, or expands past the limit for its size.
- It doesn't start with `>`, or its first record doesn't begin with the magic. Any other FASTA, such as a real genome, is not a cartridge.
- A sequence contains anything other than `A`, `C`, `G`, `T`, or isn't a whole number of bytes.
- The version, a kind or a compression method is unknown, or a chromosome appears twice.
- A declared size is inconsistent or over the limit, or the table refers to bytes that don't exist.
- The MD5 of the decoded ROM doesn't match.

MD5 detects accidental corruption and mutations. It doesn't protect against deliberate forgery.

## Version 1

Version 1 cartridges are still accepted. They have exactly one record, whose sequence holds:

| Offset | Size | Field | Value |
|---|---|---|---|
| 0 | 4 | Magic | ASCII `GBDN` |
| 4 | 1 | Version | `1` |
| 5 | 1 | Compression | `0` = none, `1` = zstd |
| 6 | 2 | Reserved | `0` |
| 8 | 4 | ROM size | Decoded size in bytes |
| 12 | 4 | Payload size | Must equal the bytes remaining after this header |
| 16 | 16 | MD5 | Digest of the decoded ROM |
| 32 | n | Payload | The ROM, compressed as declared |

## Size

Plain FASTA takes a little over 4 bytes per payload byte, including line breaks. Gzip brings that back close to the payload size, because each base carries only 2 bits. The physical mass reported by Studio assumes 5.48e-22 g per base of single-stranded DNA.

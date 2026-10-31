# GBADN format, version 1

A GBADN cartridge is a FASTA file with exactly one record. Its sequence encodes a binary container that holds a GBA ROM.

## FASTA layout

```
>GBADN1 title="GBADN DEMO" code=GDNA rom_bytes=1024 md5=fed2821399a5d92a58313b784b1a3808
CACTCAAGCACACATGAAACAAACAAAAAAAAAAAAAACAAAAAAAAACTCAAAAAAAAAAAAATTTGTC
...
```

The header line is informational, and decoders must not rely on it. `code` is `NONE` when the ROM has no game code.

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

## Container

Integers are little-endian.

| Offset | Size | Field | Value |
|---|---|---|---|
| 0 | 4 | Magic | ASCII `GBDN` |
| 4 | 1 | Version | `1` |
| 5 | 1 | Compression | `0` = none, `1` = zstd |
| 6 | 2 | Reserved | `0` |
| 8 | 4 | ROM size | Decoded size in bytes, at most 32 MiB |
| 12 | 4 | Payload size | Must equal the bytes remaining after this header |
| 16 | 16 | MD5 | Digest of the decoded ROM |
| 32 | n | Payload | The ROM, compressed as declared |

Encoders use zstd only when it makes the payload smaller than the ROM.

## Decoding

A decoder must reject a file when any of these is true:

- It doesn't start with `>`, has a second record, or doesn't begin with the magic. Any other FASTA, such as a real genome, is not a cartridge.
- The sequence contains anything other than `A`, `C`, `G`, `T`, or isn't a whole number of bytes.
- The version or compression method is unknown.
- A declared size is inconsistent or over the limit.
- The MD5 of the decoded ROM doesn't match.

MD5 detects accidental corruption and mutations. It doesn't protect against deliberate forgery.

## Size

A FASTA file takes about `(32 + payload) x 4 x 71/70` bytes plus its header line. The physical mass reported by Studio assumes 5.48e-22 g per base of single-stranded DNA.

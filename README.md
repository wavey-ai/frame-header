# Audio Frame Header Library

[![CI](https://github.com/wavey-ai/frame-header/actions/workflows/ci.yml/badge.svg)](https://github.com/wavey-ai/frame-header/actions/workflows/ci.yml)

A Rust library implementing compact and efficient binary frame headers for audio packets. The v1 header is kept for compatibility; `FrameHeaderV2` adds exact payload sizing, decoded frame counts, compact IDs, and optional packet CRC32.

## Features

- Compact 32-bit base header with optional 64-bit ID and PTS fields
- Compact v2 64-bit base header with payload size and decoded frame count
- Support for multiple encodings (PCM Signed/Float, Opus, FLAC, AAC, H264)
- Optional v2 packet CRC32 over header metadata and payload
- Efficient bit-packed fields for maximum space utilization
- WASM compatibility with special ID handling
- Comprehensive validation of audio parameters
- In-place header modification capabilities
- Field extraction without full header parsing

## V1 Header Format

The original header uses a 32-bit base format with optional 64-bit fields:

```
Base Header (32 bits):
[31-26] Magic Word (6 bits) = 0x2A
[25-24] Sample Rate (2 bits)
[23-22] Bits Per Sample (2 bits)
[21]    PTS Present Flag (1 bit)
[20]    ID Present Flag (1 bit)
[19-17] Encoding Flag (3 bits)
[16]    Endianness (1 bit)
[15-12] Channels-1 (4 bits)
[11-0]  Sample Size (12 bits)

Optional Fields:
- 64-bit ID (if ID flag set)
- 64-bit PTS (if PTS flag set)
```

### Supported Parameters

- **Encodings**: PCM (Signed/Float), Opus, FLAC, AAC, H264
- **V1 Sample Rates**: 16kHz, 44.1kHz, 48kHz, 96kHz
- **Channels**: 1-16
- **Bits Per Sample**: 16, 24, 32
- **Endianness**: Little/Big Endian
- **Sample Size**: Up to 4095 samples
- **Optional Fields**:
  - 64-bit ID
  - 64-bit PTS (Presentation Timestamp)

## V2 Header Format

`FrameHeaderV2` uses an 8-byte common base header:

```txt
Control Word (32 bits):
[31-26] Magic Word (6 bits) = 0x2B
[25-24] Version (2 bits) = 2
[23-16] Flags (8 bits)
[15-12] Encoding Flag (4 bits)
[11-8]  Sample Rate Code (4 bits)
[7-3]   Channels-1 (5 bits)
[2-0]   Bits Per Sample Code (3 bits)

Size Word (32 bits):
[31-16] Payload Size (16 bits, 0xFFFF means extended)
[15-0]  Decoded Frame Count (16 bits, 0xFFFF means extended)

Optional Fields:
- 32-bit extended payload size + 32-bit extended frame count
- 32-bit or 64-bit ID
- 64-bit PTS
- 32-bit packet CRC
```

Common LL audio packets stay small:

```txt
v1 base:                      4 bytes
v1 with ID + PTS:             20 bytes
v2 base:                      8 bytes
v2 with u32 ID + PTS:         20 bytes
v2 with u32 ID + PTS + CRC32: 24 bytes
v2 with u64 ID + PTS + CRC32: 28 bytes
```

V2 field semantics:

- **Payload Size**: exact packet payload byte length. This makes compressed Opus/AAC/FLAC streams self-delimiting.
- **Frame Count**: decoded sample frames per channel.
- **ID**: compact 32-bit ID when possible, automatically widened to 64-bit when needed.
- **PTS**: exact presentation timestamp, normally in sample frames for audio.
- **CRC32**: optional IEEE CRC32 over the encoded header prefix and payload. The stored CRC field itself is excluded from the checksum.
- **V2 Sample Rates**: 8kHz, 12kHz, 16kHz, 24kHz, 32kHz, 44.1kHz, 48kHz, 88.2kHz, 96kHz, 176.4kHz, 192kHz.

## Usage

### Creating a Header

```rust
use frame_header::{FrameHeader, EncodingFlag, Endianness};

let header = FrameHeader::new(
    EncodingFlag::PCMSigned,
    1024,                    // sample_size
    48000,                   // sample_rate
    2,                       // channels
    24,                      // bits_per_sample
    Endianness::LittleEndian,
    Some(12345),            // optional id
    Some(67890),            // optional pts
)?;
```

### Creating a V2 Header

```rust
use frame_header::{EncodingFlag, Endianness, FrameHeaderV2};

let payload = &[0x11, 0x22, 0x33];
let header = FrameHeaderV2::new(
    EncodingFlag::Opus,
    payload.len() as u32,     // exact encoded payload bytes
    960,                      // decoded frames per channel
    48000,
    2,
    0,                        // compressed payloads do not need PCM bit depth
    Endianness::LittleEndian,
    Some(7),                  // compact 32-bit ID on the wire
    Some(48_000),             // PTS in sample frames
    None,
)?.with_packet_crc32(payload)?;
```

### Encoding/Decoding

```rust
// Encode to bytes
let mut buffer = Vec::new();
header.encode(&mut buffer)?;

// Decode from bytes
let decoded = FrameHeader::decode(&mut &buffer[..])?;
```

### Modifying Headers

```rust
// Patch individual fields
FrameHeader::patch_sample_size(&mut header_bytes, 2048)?;
FrameHeader::patch_encoding(&mut header_bytes, EncodingFlag::FLAC)?;
FrameHeader::patch_sample_rate(&mut header_bytes, 96000)?;
FrameHeader::patch_channels(&mut header_bytes, 4)?;
FrameHeader::patch_bits_per_sample(&mut header_bytes, 32)?;
FrameHeader::patch_id(&mut header_bytes, Some(0xDEADBEEF))?;
FrameHeader::patch_pts(&mut header_bytes, Some(0xCAFEBABE))?;
```

### Quick Field Extraction

```rust
// Extract specific fields without full header parsing
let sample_count = FrameHeader::extract_sample_count(&header_bytes)?;
let encoding = FrameHeader::extract_encoding(&header_bytes)?;
let id = FrameHeader::extract_id(&header_bytes)?;
let pts = FrameHeader::extract_pts(&header_bytes)?;
```

### V1 Header Size

The total header size varies based on the presence of optional fields:

- Base header: 4 bytes
- With ID: 12 bytes
- With PTS: 12 bytes
- With both ID and PTS: 20 bytes

## Validation

The library performs extensive validation:

- Magic word verification (0x2A)
- Valid sample rates (44.1kHz, 48kHz, 88.2kHz, 96kHz)
- Channel count limits (1-16)
- Supported bits per sample (16, 24, 32)
- Maximum sample size (4095)
- V2 payload size and frame count extension sentinels
- Header size validation
- Encoding flag validation

## WASM Support

The library includes special handling for 64-bit IDs in WASM environments:

- IDs are serialized as strings in WASM
- Custom serialization/deserialization implementations
- Maintains compatibility across platforms

## Performance Considerations

- Bit-packed format minimizes memory usage
- Field extraction without full parsing
- In-place modification capabilities
- Efficient binary operations
- Header size adapts to optional fields

## Testing

The library includes comprehensive tests covering:

- Encoding/decoding roundtrips
- Field validation
- Boundary conditions
- WASM compatibility
- Field isolation during patching
- Optional field handling
- Edge cases and error conditions

## License

MIT

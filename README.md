# Audio Frame Header Library

A Rust library implementing a compact and efficient binary frame header format for audio and video data. The header format supports multiple encodings, sample rates, and channel configurations while maintaining a small footprint.

## Features

- Compact 32-bit base header with optional 64-bit ID and PTS fields
- Support for multiple encodings (PCM Signed/Float, Opus, FLAC, AAC, H264)
- Efficient bit-packed fields for maximum space utilization
- WASM compatibility with special ID handling
- Comprehensive validation of audio parameters
- In-place header modification capabilities
- Field extraction without full header parsing

## Header Format

The header uses a 32-bit base format with optional 64-bit fields:

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
- **Sample Rates**: 44.1kHz, 48kHz, 88.2kHz, 96kHz
- **Channels**: 1-16
- **Bits Per Sample**: 16, 24, 32
- **Endianness**: Little/Big Endian
- **Sample Size**: Up to 4095 samples
- **Optional Fields**: 
  - 64-bit ID
  - 64-bit PTS (Presentation Timestamp)

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

### Header Size

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

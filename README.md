# Audio Frame Header Library

A Rust library implementing a compact and efficient binary frame header format for audio data. The header format supports multiple audio encodings, sample rates, and channel configurations while maintaining a small footprint.

## Features

- Compact 32-bit header format with optional 64-bit ID
- Support for multiple audio encodings (PCM, Opus, FLAC, AAC)
- Efficient bit-packed fields for maximum space utilization
- WASM compatibility with special ID handling
- Comprehensive validation of audio parameters
- In-place header modification capabilities
- Field extraction without full header parsing

## Header Format

The header uses a 32-bit format with the following bit layout:

```
[31-23] Magic Word (9 bits)  = 0x155
[22-20] Encoding Flag (3 bits)
[19-18] Sample Rate (2 bits)
[17-14] Channels (4 bits)
[13-2]  Sample Size (12 bits)
[1]     Bits Per Sample (2 bits)
[0]     Endianness (1 bit)
[0]     ID Present Flag (1 bit)
[Optional] 64-bit ID
```

### Supported Parameters

- **Encodings**: PCM (Signed/Float), Opus, FLAC, AAC, H264
- **Sample Rates**: 44.1kHz, 48kHz, 88.2kHz, 96kHz
- **Channels**: 1-16
- **Bits Per Sample**: 16, 24, 32
- **Endianness**: Little/Big Endian
- **Sample Size**: Up to 4095 samples
- **Optional ID**: 64-bit identifier

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
```

### Quick Field Extraction

```rust
// Extract specific fields without full header parsing
let sample_count = FrameHeader::extract_sample_count(&header_bytes)?;
let encoding = FrameHeader::extract_encoding(&header_bytes)?;
let id = FrameHeader::extract_id(&header_bytes)?;
```

## Validation

The library performs extensive validation:

- Magic word verification
- Valid sample rates check
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

## License

MIT

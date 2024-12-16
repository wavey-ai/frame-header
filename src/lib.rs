use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum Endianness {
    LittleEndian,
    BigEndian,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum EncodingFlag {
    PCMSigned = 0,
    PCMFloat = 1,
    Opus = 2,
    FLAC = 3,
    AAC = 4,
    H264 = 5,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FrameHeader {
    encoding: EncodingFlag,
    sample_size: u16,
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
    endianness: Endianness,
    id: Option<u64>,
}

impl FrameHeader {
    // Magic word: 0x155 (binary 101010101)
    const MAGIC_WORD: u32 = 0x155;
    const MAGIC_SHIFT: u32 = 23;
    const MAGIC_MASK: u32 = 0x1FF << 23; // 9 bits

    // Field masks and shifts
    const ENCODING_SHIFT: u32 = 20;
    const ENCODING_MASK: u32 = 0x7 << 20; // 3 bits

    const SAMPLE_RATE_SHIFT: u32 = 18;
    const SAMPLE_RATE_MASK: u32 = 0x3 << 18; // 2 bits

    const CHANNELS_SHIFT: u32 = 14;
    const CHANNELS_MASK: u32 = 0xF << 14; // 4 bits

    const SAMPLE_SIZE_SHIFT: u32 = 2;
    const SAMPLE_SIZE_MASK: u32 = 0xFFF << 2; // 12 bits

    const BITS_SHIFT: u32 = 1;
    const BITS_MASK: u32 = 0x3 << 1; // 2 bits

    const ENDIAN_SHIFT: u32 = 1;
    const ENDIAN_MASK: u32 = 0x1 << 1; // 1 bit

    const ID_MASK: u32 = 0x1; // 1 bit

    const VALID_SAMPLE_RATES: [u32; 4] = [44100, 48000, 88200, 96000];
    const MAX_SAMPLE_SIZE: u16 = 0xFFF; // 4095

    pub fn new(
        encoding: EncodingFlag,
        sample_size: u16,
        sample_rate: u32,
        channels: u8,
        bits_per_sample: u8,
        endianness: Endianness,
        id: Option<u64>,
    ) -> Result<Self, String> {
        // Validate channels (1-16)
        if channels == 0 || channels > 16 {
            return Err("Channel count must be between 1 and 16".to_string());
        }

        // Validate bits per sample (only 16, 24, 32 allowed)
        match bits_per_sample {
            16 | 24 | 32 => {}
            _ => return Err("Bits per sample must be 16, 24, or 32".to_string()),
        }

        // Validate sample size (max 4095)
        if sample_size > Self::MAX_SAMPLE_SIZE {
            return Err(format!(
                "Sample size exceeds maximum value ({})",
                Self::MAX_SAMPLE_SIZE
            ));
        }

        // Validate sample rate
        if !Self::VALID_SAMPLE_RATES.contains(&sample_rate) {
            return Err(format!(
                "Invalid sample rate: {}. Must be one of: {:?}",
                sample_rate,
                Self::VALID_SAMPLE_RATES
            ));
        }

        Ok(FrameHeader {
            encoding,
            sample_size,
            sample_rate,
            channels,
            bits_per_sample,
            endianness,
            id,
        })
    }

    pub fn encode<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut header: u32 = Self::MAGIC_WORD << Self::MAGIC_SHIFT;

        // Encoding flag (3 bits)
        header |= (self.encoding as u32) << Self::ENCODING_SHIFT;

        // Sample rate code (2 bits)
        let sample_rate_code = match self.sample_rate {
            44100 => 0,
            48000 => 1,
            88200 => 2,
            96000 => 3,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid sample rate",
                ))
            }
        };
        header |= sample_rate_code << Self::SAMPLE_RATE_SHIFT;

        // Channels (4 bits)
        header |= ((self.channels - 1) as u32) << Self::CHANNELS_SHIFT;

        // Sample size (12 bits)
        header |= (self.sample_size as u32) << Self::SAMPLE_SIZE_SHIFT;

        // Bits per sample (2 bits)
        let bits_code = match self.bits_per_sample {
            16 => 0,
            24 => 1,
            32 => 2,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid bits per sample",
                ))
            }
        };
        header |= bits_code << Self::BITS_SHIFT;

        // Endianness (1 bit)
        header |= (self.endianness as u32) << Self::ENDIAN_SHIFT;

        // ID present flag (1 bit)
        header |= self.id.is_some() as u32;

        writer.write_all(&header.to_be_bytes())?;

        if let Some(id) = self.id {
            writer.write_all(&id.to_be_bytes())?;
        }

        Ok(())
    }

    pub fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut header_bytes = [0u8; 4];
        reader.read_exact(&mut header_bytes)?;
        let header = u32::from_be_bytes(header_bytes);

        // Verify magic word
        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid header magic word",
            ));
        }

        let encoding = match (header & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT {
            0 => EncodingFlag::PCMSigned,
            1 => EncodingFlag::PCMFloat,
            2 => EncodingFlag::Opus,
            3 => EncodingFlag::FLAC,
            4 => EncodingFlag::AAC,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid encoding flag",
                ))
            }
        };

        let sample_rate = match (header & Self::SAMPLE_RATE_MASK) >> Self::SAMPLE_RATE_SHIFT {
            0 => 44100,
            1 => 48000,
            2 => 88200,
            3 => 96000,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid sample rate code",
                ))
            }
        };

        let channels = (((header & Self::CHANNELS_MASK) >> Self::CHANNELS_SHIFT) + 1) as u8;
        let sample_size = ((header & Self::SAMPLE_SIZE_MASK) >> Self::SAMPLE_SIZE_SHIFT) as u16;

        let bits_per_sample = match (header & Self::BITS_MASK) >> Self::BITS_SHIFT {
            0 => 16,
            1 => 24,
            2 => 32,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid bits per sample code",
                ))
            }
        };

        let endianness = if (header & Self::ENDIAN_MASK) >> Self::ENDIAN_SHIFT == 0 {
            Endianness::LittleEndian
        } else {
            Endianness::BigEndian
        };

        let has_id = header & Self::ID_MASK == 1;
        let id = if has_id {
            let mut id_bytes = [0u8; 8];
            reader.read_exact(&mut id_bytes)?;
            Some(u64::from_be_bytes(id_bytes))
        } else {
            None
        };

        Ok(FrameHeader {
            encoding,
            sample_size,
            sample_rate,
            channels,
            bits_per_sample,
            endianness,
            id,
        })
    }

    pub fn validate_header(header_bytes: &[u8]) -> Result<bool, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        // Check magic word
        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Ok(false);
        }

        // Validate encoding (3 bits)
        let encoding = (header & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT;
        if encoding > 4 {
            return Ok(false);
        }

        // Validate sample rate (2 bits)
        let sample_rate_code = (header & Self::SAMPLE_RATE_MASK) >> Self::SAMPLE_RATE_SHIFT;
        if sample_rate_code > 3 {
            return Ok(false);
        }

        // Validate channels (4 bits)
        let channels = (((header & Self::CHANNELS_MASK) >> Self::CHANNELS_SHIFT) + 1) as u8;
        if channels == 0 || channels > 16 {
            return Ok(false);
        }

        // Validate bits per sample (2 bits)
        let bits_code = (header & Self::BITS_MASK) >> Self::BITS_SHIFT;
        if bits_code > 2 {
            return Ok(false);
        }

        Ok(true)
    }

    // Getter methods
    pub fn encoding(&self) -> &EncodingFlag {
        &self.encoding
    }

    pub fn sample_size(&self) -> u16 {
        self.sample_size
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u8 {
        self.channels
    }

    pub fn bits_per_sample(&self) -> u8 {
        self.bits_per_sample
    }

    pub fn endianness(&self) -> &Endianness {
        &self.endianness
    }

    pub fn id(&self) -> Option<u64> {
        self.id
    }

    pub fn size(&self) -> usize {
        if self.id.is_some() {
            12 // 4 bytes header + 8 bytes id
        } else {
            4 // Just header
        }
    }

    // Patch methods
    pub fn patch_bits_per_sample(header_bytes: &mut [u8], bits: u8) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        let bits_code = match bits {
            16 => 0,
            24 => 1,
            32 => 2,
            _ => return Err("Bits per sample must be 16, 24, or 32".to_string()),
        };

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::BITS_MASK;
        header |= (bits_code << Self::BITS_SHIFT) & Self::BITS_MASK;
        header_bytes[..4].copy_from_slice(&header.to_be_bytes());
        Ok(())
    }

    pub fn patch_sample_size(header_bytes: &mut [u8], new_sample_size: u16) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        if new_sample_size > Self::MAX_SAMPLE_SIZE {
            return Err(format!(
                "Sample size exceeds maximum value ({})",
                Self::MAX_SAMPLE_SIZE
            ));
        }

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::SAMPLE_SIZE_MASK;
        header |= ((new_sample_size as u32) << Self::SAMPLE_SIZE_SHIFT) & Self::SAMPLE_SIZE_MASK;
        header_bytes[..4].copy_from_slice(&header.to_be_bytes());
        Ok(())
    }

    pub fn patch_encoding(header_bytes: &mut [u8], encoding: EncodingFlag) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::ENCODING_MASK;
        header |= ((encoding as u32) << Self::ENCODING_SHIFT) & Self::ENCODING_MASK;
        header_bytes[..4].copy_from_slice(&header.to_be_bytes());
        Ok(())
    }

    pub fn patch_sample_rate(header_bytes: &mut [u8], sample_rate: u32) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        let rate_code = match sample_rate {
            44100 => 0,
            48000 => 1,
            88200 => 2,
            96000 => 3,
            _ => {
                return Err(format!(
                    "Invalid sample rate: {}. Must be one of: {:?}",
                    sample_rate,
                    Self::VALID_SAMPLE_RATES
                ))
            }
        };

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::SAMPLE_RATE_MASK;
        header |= (rate_code << Self::SAMPLE_RATE_SHIFT) & Self::SAMPLE_RATE_MASK;
        header_bytes[..4].copy_from_slice(&header.to_be_bytes());
        Ok(())
    }

    pub fn patch_channels(header_bytes: &mut [u8], channels: u8) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        if channels == 0 || channels > 16 {
            return Err("Channel count must be between 1 and 16".to_string());
        }

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::CHANNELS_MASK;
        header |= (((channels - 1) as u32) << Self::CHANNELS_SHIFT) & Self::CHANNELS_MASK;
        header_bytes[..4].copy_from_slice(&header.to_be_bytes());
        Ok(())
    }

    /// Patch the ID present flag and optionally the ID itself in an existing header
    pub fn patch_id(header_bytes: &mut [u8], id: Option<u64>) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::ID_MASK;
        header |= (id.is_some() as u32) & Self::ID_MASK;
        header_bytes[..4].copy_from_slice(&header.to_be_bytes());

        // If we're adding an ID, append it after the header
        if let Some(id_value) = id {
            if header_bytes.len() < 12 {
                return Err("Buffer too small to add ID".to_string());
            }
            header_bytes[4..12].copy_from_slice(&id_value.to_be_bytes());
        }

        Ok(())
    }

    pub fn extract_sample_count(header_bytes: &[u8]) -> Result<u16, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        // Verify magic word first
        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err("Invalid magic word".to_string());
        }

        Ok(((header & Self::SAMPLE_SIZE_MASK) >> Self::SAMPLE_SIZE_SHIFT) as u16)
    }

    /// Extract just the ID if present from a header without fully decoding it
    pub fn extract_id(header_bytes: &[u8]) -> Result<Option<u64>, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        // Verify magic word first
        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err("Invalid magic word".to_string());
        }

        if header & Self::ID_MASK == 0 {
            return Ok(None);
        }

        if header_bytes.len() < 12 {
            return Err("Header indicates ID present but buffer too small".to_string());
        }

        Ok(Some(u64::from_be_bytes(
            header_bytes[4..12].try_into().unwrap(),
        )))
    }

    pub fn extract_encoding(header_bytes: &[u8]) -> Result<EncodingFlag, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        // Verify magic word first
        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err("Invalid magic word".to_string());
        }

        match (header & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT {
            0 => Ok(EncodingFlag::PCMSigned),
            1 => Ok(EncodingFlag::PCMFloat),
            2 => Ok(EncodingFlag::Opus),
            3 => Ok(EncodingFlag::FLAC),
            4 => Ok(EncodingFlag::AAC),
            _ => Err("Invalid encoding flag".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_header() -> Vec<u8> {
        let header = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            None,
        )
        .unwrap();
        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer
    }

    fn create_header_with_id() -> Vec<u8> {
        let header = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            Some(0x1234567890ABCDEF),
        )
        .unwrap();
        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn test_magic_word_validation() {
        let valid_header = create_test_header();
        assert!(FrameHeader::validate_header(&valid_header).unwrap());

        // Test invalid magic word
        let mut invalid_magic = valid_header.clone();
        invalid_magic[0] = 0; // Corrupt magic word
        assert!(!FrameHeader::validate_header(&invalid_magic).unwrap());

        // Test truncated header
        let short_header = vec![0; 2];
        assert!(FrameHeader::validate_header(&short_header).is_err());
    }

    #[test]
    fn test_constructor_validation() {
        // Test valid parameters
        assert!(FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            None,
        )
        .is_ok());

        // Test invalid sample size
        assert!(FrameHeader::new(
            EncodingFlag::PCMSigned,
            5000, // Too large
            48000,
            2,
            24,
            Endianness::LittleEndian,
            None,
        )
        .is_err());

        // Test invalid sample rate
        assert!(FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            192000, // Invalid rate
            2,
            24,
            Endianness::LittleEndian,
            None,
        )
        .is_err());

        // Test invalid channels
        assert!(FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            17, // Too many channels
            24,
            Endianness::LittleEndian,
            None,
        )
        .is_err());

        // Test invalid bits per sample
        assert!(FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            20, // Invalid bit depth
            Endianness::LittleEndian,
            None,
        )
        .is_err());
    }

    #[test]
    fn test_encoding_roundtrip() {
        let original = FrameHeader::new(
            EncodingFlag::Opus,
            2048,
            48000,
            8,
            16,
            Endianness::LittleEndian,
            Some(0xDEADBEEF),
        )
        .unwrap();

        let mut buffer = Vec::new();
        original.encode(&mut buffer).unwrap();

        let decoded = FrameHeader::decode(&mut &buffer[..]).unwrap();

        assert_eq!(*decoded.encoding(), *original.encoding());
        assert_eq!(decoded.sample_size(), original.sample_size());
        assert_eq!(decoded.sample_rate(), original.sample_rate());
        assert_eq!(decoded.channels(), original.channels());
        assert_eq!(decoded.bits_per_sample(), original.bits_per_sample());
        assert_eq!(*decoded.endianness(), *original.endianness());
        assert_eq!(decoded.id(), original.id());
    }

    #[test]
    fn test_patch_operations() {
        let mut header_bytes = create_test_header();

        // Test sample size patching
        assert!(FrameHeader::patch_sample_size(&mut header_bytes, 2048).is_ok());
        let updated = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(updated.sample_size(), 2048);

        // Test encoding patching
        assert!(FrameHeader::patch_encoding(&mut header_bytes, EncodingFlag::FLAC).is_ok());
        let updated = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(*updated.encoding(), EncodingFlag::FLAC);

        // Test sample rate patching
        assert!(FrameHeader::patch_sample_rate(&mut header_bytes, 96000).is_ok());
        let updated = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(updated.sample_rate(), 96000);

        // Test bits per sample patching
        assert!(FrameHeader::patch_bits_per_sample(&mut header_bytes, 32).is_ok());
        let updated = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(updated.bits_per_sample(), 32);

        // Test channels patching
        assert!(FrameHeader::patch_channels(&mut header_bytes, 16).is_ok());
        let updated = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(updated.channels(), 16);
    }

    #[test]
    fn test_patch_validation() {
        let mut header_bytes = create_test_header();

        // Test invalid sample size
        assert!(FrameHeader::patch_sample_size(&mut header_bytes, 5000).is_err());

        // Test invalid sample rate
        assert!(FrameHeader::patch_sample_rate(&mut header_bytes, 192000).is_err());

        // Test invalid channels
        assert!(FrameHeader::patch_channels(&mut header_bytes, 17).is_err());
        assert!(FrameHeader::patch_channels(&mut header_bytes, 0).is_err());

        // Test invalid bits per sample
        assert!(FrameHeader::patch_bits_per_sample(&mut header_bytes, 20).is_err());
    }

    #[test]
    fn test_field_preservation() {
        let mut header_bytes = create_test_header();
        let original = FrameHeader::decode(&mut &header_bytes[..]).unwrap();

        // Modify just sample size and verify other fields remain unchanged
        FrameHeader::patch_sample_size(&mut header_bytes, 2048).unwrap();
        let updated = FrameHeader::decode(&mut &header_bytes[..]).unwrap();

        assert_eq!(updated.sample_size(), 2048); // Changed
        assert_eq!(updated.encoding(), original.encoding()); // Preserved
        assert_eq!(updated.sample_rate(), original.sample_rate()); // Preserved
        assert_eq!(updated.channels(), original.channels()); // Preserved
        assert_eq!(updated.bits_per_sample(), original.bits_per_sample()); // Preserved
        assert_eq!(updated.endianness(), original.endianness()); // Preserved
        assert_eq!(updated.id(), original.id()); // Preserved
    }

    #[test]
    fn test_id_handling() {
        let header_bytes = create_header_with_id();
        assert_eq!(header_bytes.len(), 12); // 4 bytes header + 8 bytes ID

        let decoded = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(decoded.id(), Some(0x1234567890ABCDEF));
        assert_eq!(decoded.size(), 12);

        // Test patching ID
        let mut header_bytes = create_test_header();
        assert_eq!(header_bytes.len(), 4); // No ID initially

        let mut extended_bytes = vec![0; 12];
        extended_bytes[..4].copy_from_slice(&header_bytes);

        assert!(FrameHeader::patch_id(&mut extended_bytes, Some(0xDEADBEEF)).is_ok());
        let updated = FrameHeader::decode(&mut &extended_bytes[..]).unwrap();
        assert_eq!(updated.id(), Some(0xDEADBEEF));
    }

    #[test]
    fn test_quick_extract() {
        let header_bytes = create_test_header();

        // Test sample count extraction
        let sample_count = FrameHeader::extract_sample_count(&header_bytes).unwrap();
        assert_eq!(sample_count, 1024);

        // Test encoding extraction
        let encoding = FrameHeader::extract_encoding(&header_bytes).unwrap();
        assert_eq!(encoding, EncodingFlag::PCMSigned);

        // Test with invalid magic word
        let mut invalid_header = header_bytes.clone();
        invalid_header[0] = 0; // Corrupt magic word
        assert!(FrameHeader::extract_sample_count(&invalid_header).is_err());
        assert!(FrameHeader::extract_encoding(&invalid_header).is_err());
    }

    #[test]
    fn test_extract_id() {
        // Test header with ID
        let header_with_id = create_header_with_id();
        let id = FrameHeader::extract_id(&header_with_id).unwrap();
        assert_eq!(id, Some(0x1234567890ABCDEF));

        // Test header without ID
        let header_no_id = create_test_header();
        let id = FrameHeader::extract_id(&header_no_id).unwrap();
        assert_eq!(id, None);

        // Test invalid cases
        let mut invalid_header = header_with_id.clone();
        invalid_header[0] = 0; // Corrupt magic word
        assert!(FrameHeader::extract_id(&invalid_header).is_err());

        // Test truncated header with ID flag set
        let mut truncated = header_with_id[..4].to_vec();
        assert!(FrameHeader::extract_id(&truncated).is_err());
    }
}

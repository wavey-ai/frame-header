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

    #[cfg(not(target_arch = "wasm32"))]
    id: Option<u64>,
    pts: Option<u64>,

    #[cfg(target_arch = "wasm32")]
    #[serde(
        serialize_with = "serialize_id_wasm",
        deserialize_with = "deserialize_id_wasm"
    )]
    id: Option<u64>,
}

#[cfg(target_arch = "wasm32")]
fn serialize_id_wasm<S>(id: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match id {
        Some(value) => serializer.serialize_some(&value.to_string()),
        None => serializer.serialize_none(),
    }
}

#[cfg(target_arch = "wasm32")]
fn deserialize_id_wasm<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let id: Option<String> = Option::deserialize(deserializer)?;
    match id {
        Some(id_str) => id_str.parse::<u64>().map(Some).map_err(D::Error::custom),
        None => Ok(None),
    }
}

impl FrameHeader {
    const MAGIC_WORD: u32 = 0x2A;
    const MAGIC_SHIFT: u32 = 26;
    const MAGIC_MASK: u32 = 0x3F << 26;

    const SAMPLE_RATE_SHIFT: u32 = 24;
    const SAMPLE_RATE_MASK: u32 = 0x3 << 24;

    const BITS_SHIFT: u32 = 22;
    const BITS_MASK: u32 = 0x3 << 22;

    const PTS_SHIFT: u32 = 21;
    const PTS_MASK: u32 = 0x1 << 21;

    const ID_SHIFT: u32 = 20;
    const ID_MASK: u32 = 0x1 << 20;

    const ENCODING_SHIFT: u32 = 17;
    const ENCODING_MASK: u32 = 0x7 << 17;

    const ENDIAN_SHIFT: u32 = 16;
    const ENDIAN_MASK: u32 = 0x1 << 16;

    const CHANNELS_SHIFT: u32 = 12;
    const CHANNELS_MASK: u32 = 0xF << 12;

    const SAMPLE_SIZE_MASK: u32 = 0xFFF;

    const VALID_SAMPLE_RATES: [u32; 4] = [16000, 44100, 48000, 96000];
    const MAX_SAMPLE_SIZE: u16 = 0xFFF;

    pub fn new(
        encoding: EncodingFlag,
        sample_size: u16,
        sample_rate: u32,
        channels: u8,
        bits_per_sample: u8,
        endianness: Endianness,
        id: Option<u64>,
        pts: Option<u64>,
    ) -> Result<Self, String> {
        if channels == 0 || channels > 16 {
            return Err("Channel count must be between 1 and 16".to_string());
        }

        match bits_per_sample {
            16 | 24 | 32 => {}
            _ => return Err("Bits per sample must be 16, 24, or 32".to_string()),
        }

        if sample_size > Self::MAX_SAMPLE_SIZE {
            return Err(format!(
                "Sample size exceeds maximum value ({})",
                Self::MAX_SAMPLE_SIZE
            ));
        }

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
            pts,
        })
    }

    pub fn encode<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut header: u32 = Self::MAGIC_WORD << Self::MAGIC_SHIFT;

        let sample_rate_code = match self.sample_rate {
            16000 => 0,
            44100 => 1,
            48000 => 2,
            96000 => 3,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid sample rate",
                ))
            }
        };
        header |= sample_rate_code << Self::SAMPLE_RATE_SHIFT;

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

        header |= (self.pts.is_some() as u32) << Self::PTS_SHIFT;
        header |= (self.id.is_some() as u32) << Self::ID_SHIFT;
        header |= (self.encoding as u32) << Self::ENCODING_SHIFT;
        header |= (self.endianness as u32) << Self::ENDIAN_SHIFT;
        header |= ((self.channels - 1) as u32) << Self::CHANNELS_SHIFT;
        header |= self.sample_size as u32;

        writer.write_all(&header.to_be_bytes())?;

        if let Some(id) = self.id {
            writer.write_all(&id.to_be_bytes())?;
        }

        if let Some(pts) = self.pts {
            writer.write_all(&pts.to_be_bytes())?;
        }

        Ok(())
    }

    pub fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut header_bytes = [0u8; 4];
        reader.read_exact(&mut header_bytes)?;
        let header = u32::from_be_bytes(header_bytes);

        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid header magic word",
            ));
        }

        let sample_rate = match (header & Self::SAMPLE_RATE_MASK) >> Self::SAMPLE_RATE_SHIFT {
            0 => 16000,
            1 => 44100,
            2 => 48000,
            3 => 96000,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid sample rate code",
                ))
            }
        };

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

        let has_pts = (header & Self::PTS_MASK) >> Self::PTS_SHIFT == 1;
        let has_id = (header & Self::ID_MASK) >> Self::ID_SHIFT == 1;

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

        let endianness = if (header & Self::ENDIAN_MASK) >> Self::ENDIAN_SHIFT == 0 {
            Endianness::LittleEndian
        } else {
            Endianness::BigEndian
        };

        let channels = (((header & Self::CHANNELS_MASK) >> Self::CHANNELS_SHIFT) + 1) as u8;
        let sample_size = (header & Self::SAMPLE_SIZE_MASK) as u16;

        let id = if has_id {
            let mut id_bytes = [0u8; 8];
            reader.read_exact(&mut id_bytes)?;
            Some(u64::from_be_bytes(id_bytes))
        } else {
            None
        };

        let pts = if has_pts {
            let mut pts_bytes = [0u8; 8];
            reader.read_exact(&mut pts_bytes)?;
            Some(u64::from_be_bytes(pts_bytes))
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
            pts,
        })
    }

    pub fn validate_header(header_bytes: &[u8]) -> Result<bool, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Ok(false);
        }

        let encoding = (header & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT;
        if encoding > 4 {
            return Ok(false);
        }

        let sample_rate_code = (header & Self::SAMPLE_RATE_MASK) >> Self::SAMPLE_RATE_SHIFT;
        if sample_rate_code > 3 {
            return Ok(false);
        }

        let channels = (((header & Self::CHANNELS_MASK) >> Self::CHANNELS_SHIFT) + 1) as u8;
        if channels == 0 || channels > 16 {
            return Ok(false);
        }

        let bits_code = (header & Self::BITS_MASK) >> Self::BITS_SHIFT;
        if bits_code > 2 {
            return Ok(false);
        }

        Ok(true)
    }

    pub fn size(&self) -> usize {
        4 + // Base header
        (self.id.is_some() as usize) * 8 + // Optional ID
        (self.pts.is_some() as usize) * 8 // Optional PTS
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

    pub fn pts(&self) -> Option<u64> {
        self.pts
    }

    // Extract methods
    pub fn extract_sample_count(header_bytes: &[u8]) -> Result<u16, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err("Invalid magic word".to_string());
        }

        Ok((header & Self::SAMPLE_SIZE_MASK) as u16)
    }

    pub fn extract_encoding(header_bytes: &[u8]) -> Result<EncodingFlag, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

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

    pub fn extract_id(header_bytes: &[u8]) -> Result<Option<u64>, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err("Invalid magic word".to_string());
        }

        if (header & Self::ID_MASK) >> Self::ID_SHIFT == 0 {
            return Ok(None);
        }

        if header_bytes.len() < 12 {
            return Err("Header indicates ID present but buffer too small".to_string());
        }

        Ok(Some(u64::from_be_bytes(
            header_bytes[4..12].try_into().unwrap(),
        )))
    }

    pub fn extract_pts(header_bytes: &[u8]) -> Result<Option<u64>, String> {
        if header_bytes.len() < 4 {
            return Err("Header too small".to_string());
        }

        let header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());

        if (header & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err("Invalid magic word".to_string());
        }

        let has_pts = (header & Self::PTS_MASK) >> Self::PTS_SHIFT == 1;
        if !has_pts {
            return Ok(None);
        }

        let has_id = (header & Self::ID_MASK) >> Self::ID_SHIFT == 1;
        let pts_offset = 4 + if has_id { 8 } else { 0 };

        if header_bytes.len() < pts_offset + 8 {
            return Err("Header indicates PTS present but buffer too small".to_string());
        }

        Ok(Some(u64::from_be_bytes(
            header_bytes[pts_offset..pts_offset + 8].try_into().unwrap(),
        )))
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
        header |= new_sample_size as u32;
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
            16000 => 0,
            44100 => 1,
            48000 => 2,
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

    pub fn patch_id(header_bytes: &mut [u8], id: Option<u64>) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::ID_MASK;
        header |= ((id.is_some() as u32) << Self::ID_SHIFT) & Self::ID_MASK;
        header_bytes[..4].copy_from_slice(&header.to_be_bytes());

        if let Some(id_value) = id {
            if header_bytes.len() < 12 {
                return Err("Buffer too small to add ID".to_string());
            }
            header_bytes[4..12].copy_from_slice(&id_value.to_be_bytes());
        }

        Ok(())
    }

    pub fn patch_pts(header_bytes: &mut [u8], pts: Option<u64>) -> Result<(), String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid header".to_string());
        }

        let mut header = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        header &= !Self::PTS_MASK;
        header |= ((pts.is_some() as u32) << Self::PTS_SHIFT) & Self::PTS_MASK;

        let has_id = (header & Self::ID_MASK) >> Self::ID_SHIFT == 1;
        let pts_offset = 4 + if has_id { 8 } else { 0 };

        if let Some(pts_value) = pts {
            if header_bytes.len() < pts_offset + 8 {
                return Err("Buffer too small to add PTS".to_string());
            }
            header_bytes[pts_offset..pts_offset + 8].copy_from_slice(&pts_value.to_be_bytes());
        }

        header_bytes[..4].copy_from_slice(&header.to_be_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_header() -> Vec<u8> {
        let header = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            None,
            None,
        )
        .unwrap();
        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer
    }

    fn create_header_with_pts() -> Vec<u8> {
        let header = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            None,
            Some(0x1234567890ABCDEF),
        )
        .unwrap();
        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer
    }

    fn create_header_with_id_and_pts() -> Vec<u8> {
        let header = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            Some(0xDEADBEEF),
            Some(0xFEEDFACE),
        )
        .unwrap();
        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn test_pts_handling() {
        // Test header with PTS
        let header_bytes = create_header_with_pts();
        assert_eq!(header_bytes.len(), 12); // 4 bytes header + 8 bytes PTS

        let decoded = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(decoded.pts(), Some(0x1234567890ABCDEF));
        assert_eq!(decoded.size(), 12);

        // Test header with both ID and PTS
        let header_bytes = create_header_with_id_and_pts();
        assert_eq!(header_bytes.len(), 20); // 4 bytes header + 8 bytes ID + 8 bytes PTS

        let decoded = FrameHeader::decode(&mut &header_bytes[..]).unwrap();
        assert_eq!(decoded.id(), Some(0xDEADBEEF));
        assert_eq!(decoded.pts(), Some(0xFEEDFACE));
        assert_eq!(decoded.size(), 20);

        // Test patching PTS
        let mut header_bytes = create_test_header();
        assert_eq!(header_bytes.len(), 4); // No PTS initially

        let mut extended_bytes = vec![0; 12];
        extended_bytes[..4].copy_from_slice(&header_bytes);

        assert!(FrameHeader::patch_pts(&mut extended_bytes, Some(0xCAFEBABE)).is_ok());
        let updated = FrameHeader::decode(&mut &extended_bytes[..]).unwrap();
        assert_eq!(updated.pts(), Some(0xCAFEBABE));
    }

    #[test]
    fn test_extract_pts() {
        // Test header with PTS
        let header_with_pts = create_header_with_pts();
        let pts = FrameHeader::extract_pts(&header_with_pts).unwrap();
        assert_eq!(pts, Some(0x1234567890ABCDEF));

        // Test header without PTS
        let header_no_pts = create_test_header();
        let pts = FrameHeader::extract_pts(&header_no_pts).unwrap();
        assert_eq!(pts, None);

        // Test invalid cases
        let mut invalid_header = header_with_pts.clone();
        invalid_header[0] = 0; // Corrupt magic word
        assert!(FrameHeader::extract_pts(&invalid_header).is_err());

        // Test truncated header with PTS flag set
        let truncated = header_with_pts[..4].to_vec();
        assert!(FrameHeader::extract_pts(&truncated).is_err());
    }

    #[test]
    fn test_encoding_roundtrip_with_pts() {
        let original = FrameHeader::new(
            EncodingFlag::Opus,
            2048,
            48000,
            8,
            16,
            Endianness::LittleEndian,
            Some(0xDEADBEEF),
            Some(0xCAFEBABE),
        )
        .unwrap();

        let mut buffer = Vec::new();
        original.encode(&mut buffer).unwrap();

        let decoded = FrameHeader::decode(&mut &buffer[..]).unwrap();

        assert_eq!(decoded.pts(), original.pts());
        assert_eq!(decoded.id(), original.id());
        assert_eq!(decoded.size(), original.size());
        assert_eq!(buffer.len(), decoded.size());
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

        // Test PTS patching
        let mut extended_bytes = vec![0; 20]; // Enough space for header + id + pts
        extended_bytes[..header_bytes.len()].copy_from_slice(&header_bytes);
        assert!(FrameHeader::patch_pts(&mut extended_bytes, Some(0xCAFEBABE)).is_ok());
        let updated = FrameHeader::decode(&mut &extended_bytes[..]).unwrap();
        assert_eq!(updated.pts(), Some(0xCAFEBABE));
    }

    #[test]
    fn test_extract_operations() {
        let header_bytes = create_header_with_id_and_pts();

        assert_eq!(
            FrameHeader::extract_sample_count(&header_bytes).unwrap(),
            1024
        );
        assert_eq!(
            FrameHeader::extract_encoding(&header_bytes).unwrap(),
            EncodingFlag::PCMSigned
        );
        assert_eq!(
            FrameHeader::extract_id(&header_bytes).unwrap(),
            Some(0xDEADBEEF)
        );
        assert_eq!(
            FrameHeader::extract_pts(&header_bytes).unwrap(),
            Some(0xFEEDFACE)
        );

        // Test with invalid header
        let mut invalid_header = header_bytes.clone();
        invalid_header[0] = 0; // Corrupt magic word
        assert!(FrameHeader::extract_sample_count(&invalid_header).is_err());
        assert!(FrameHeader::extract_encoding(&invalid_header).is_err());
        assert!(FrameHeader::extract_id(&invalid_header).is_err());
        assert!(FrameHeader::extract_pts(&invalid_header).is_err());
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
    fn test_sample_size_extraction() {
        let header = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            None,
            None,
        )
        .unwrap();

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();

        let extracted = FrameHeader::extract_sample_count(&buffer).unwrap();
        assert_eq!(extracted, 1024, "Sample size extraction failed");

        // Now test with a decoded header to verify consistency
        let decoded = FrameHeader::decode(&mut &buffer[..]).unwrap();
        assert_eq!(decoded.sample_size(), 1024, "Sample size decode failed");
    }

    //This test ensures field boundaries by setting each field to its maximum value and verifying no corruption.
    #[test]
    fn test_bit_layout() {
        let header = FrameHeader::new(
            EncodingFlag::PCMSigned, // 000
            0xFFF,                   // 111111111111
            48000,                   // 01
            16,                      // 1111
            32,                      // 10
            Endianness::BigEndian,   // 1
            Some(1),                 // 1
            Some(1),                 // 1
        )
        .unwrap();

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        let decoded = FrameHeader::decode(&mut &buffer[..]).unwrap();

        // Verify max values are preserved
        assert_eq!(decoded.sample_size(), 0xFFF);
        assert_eq!(decoded.channels(), 16);
        assert_eq!(decoded.bits_per_sample(), 32);
        assert_eq!(decoded.endianness(), &Endianness::BigEndian);
        assert!(decoded.id().is_some());
        assert!(decoded.pts().is_some());
    }
    #[test]
    fn test_valid_opus_and_flac_sample_sizes_with_varied_pts_and_ids() {
        let opus_sample_sizes = [80, 160, 240, 480, 960, 1920, 2880];
        let flac_sample_sizes = [512, 1024, 2048];
        let sample_rates = [16000, 44100, 48000, 96000];
        let channels_list = [1, 2, 8, 16];
        let bits_list = [16, 24, 32];
        let endianness_list = [Endianness::LittleEndian, Endianness::BigEndian];
        let pts_values = [
            1_670_000_000_000_000,
            1_671_000_000_000_000,
            1_672_000_000_000_000,
            1_673_000_000_000_000,
            1_674_000_000_000_000,
            1_675_000_000_000_000,
        ];
        let id_values = [
            0xFFFFFFFFFFFFFFFF,
            0x0123456789ABCDEF,
            0xDEADBEEFDEADBEEF,
            0,
            1,
            42,
        ];
        for &encoding in &[EncodingFlag::Opus, EncodingFlag::FLAC] {
            let sample_sizes: &[u16] = match encoding {
                EncodingFlag::Opus => &opus_sample_sizes,
                EncodingFlag::FLAC => &flac_sample_sizes,
                _ => continue,
            };
            for &sample_size in sample_sizes {
                for &sample_rate in &sample_rates {
                    for &channels in &channels_list {
                        for &bits in &bits_list {
                            for &endianness in &endianness_list {
                                for &id_val in &id_values {
                                    for &pts_val in &pts_values {
                                        let header = FrameHeader::new(
                                            encoding,
                                            sample_size,
                                            sample_rate,
                                            channels,
                                            bits,
                                            endianness,
                                            Some(id_val),
                                            Some(pts_val),
                                        );

                                        assert!(
                                        header.is_ok(),
                                        "Failed to create header for encoding: {:?}, sample_size: {}, sample_rate: {}, channels: {}, bits: {}, endianness: {:?}, id: {:?}, pts: {:?}",
                                        encoding, sample_size, sample_rate, channels, bits, endianness, id_val, pts_val
                                    );

                                        let header = header.unwrap();
                                        let mut buffer = Vec::new();

                                        assert!(
                                        header.encode(&mut buffer).is_ok(),
                                        "Failed to encode header for encoding: {:?}, sample_size: {}, sample_rate: {}, channels: {}, bits: {}, endianness: {:?}, id: {:?}, pts: {:?}",
                                        encoding, sample_size, sample_rate, channels, bits, endianness, id_val, pts_val
                                    );

                                        let decoded = FrameHeader::decode(&mut &buffer[..]);

                                        assert!(
                                        decoded.is_ok(),
                                        "Failed to decode header for encoding: {:?}, sample_size: {}, sample_rate: {}, channels: {}, bits: {}, endianness: {:?}, id: {:?}, pts: {:?}",
                                        encoding, sample_size, sample_rate, channels, bits, endianness, id_val, pts_val
                                    );

                                        let decoded = decoded.unwrap();
                                        assert_eq!(*decoded.encoding(), encoding);
                                        assert_eq!(decoded.sample_size(), sample_size);
                                        assert_eq!(decoded.sample_rate(), sample_rate);
                                        assert_eq!(decoded.channels(), channels);
                                        assert_eq!(decoded.bits_per_sample(), bits);
                                        assert_eq!(*decoded.endianness(), endianness);
                                        assert_eq!(decoded.id(), Some(id_val));
                                        assert_eq!(decoded.pts(), Some(pts_val));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_patch_field_isolation() {
        // Create a header with known values for all fields
        let original = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            4,
            24,
            Endianness::LittleEndian,
            Some(0xDEADBEEF),
            Some(0xCAFEBABE),
        )
        .unwrap();

        let mut buffer = Vec::new();
        original.encode(&mut buffer).unwrap();

        // Test sample size patching
        let mut test_buffer = buffer.clone();
        FrameHeader::patch_sample_size(&mut test_buffer, 2048).unwrap();
        let updated = FrameHeader::decode(&mut &test_buffer[..]).unwrap();
        assert_eq!(updated.sample_size(), 2048); // Changed field
        assert_eq!(*updated.encoding(), *original.encoding());
        assert_eq!(updated.sample_rate(), original.sample_rate());
        assert_eq!(updated.channels(), original.channels());
        assert_eq!(updated.bits_per_sample(), original.bits_per_sample());
        assert_eq!(*updated.endianness(), *original.endianness());
        assert_eq!(updated.id(), original.id());
        assert_eq!(updated.pts(), original.pts());

        // Test encoding patching
        let mut test_buffer = buffer.clone();
        FrameHeader::patch_encoding(&mut test_buffer, EncodingFlag::FLAC).unwrap();
        let updated = FrameHeader::decode(&mut &test_buffer[..]).unwrap();
        assert_eq!(*updated.encoding(), EncodingFlag::FLAC); // Changed field
        assert_eq!(updated.sample_size(), original.sample_size());
        assert_eq!(updated.sample_rate(), original.sample_rate());
        assert_eq!(updated.channels(), original.channels());
        assert_eq!(updated.bits_per_sample(), original.bits_per_sample());
        assert_eq!(*updated.endianness(), *original.endianness());
        assert_eq!(updated.id(), original.id());
        assert_eq!(updated.pts(), original.pts());

        // Test sample rate patching
        let mut test_buffer = buffer.clone();
        FrameHeader::patch_sample_rate(&mut test_buffer, 96000).unwrap();
        let updated = FrameHeader::decode(&mut &test_buffer[..]).unwrap();
        assert_eq!(updated.sample_rate(), 96000); // Changed field
        assert_eq!(*updated.encoding(), *original.encoding());
        assert_eq!(updated.sample_size(), original.sample_size());
        assert_eq!(updated.channels(), original.channels());
        assert_eq!(updated.bits_per_sample(), original.bits_per_sample());
        assert_eq!(*updated.endianness(), *original.endianness());
        assert_eq!(updated.id(), original.id());
        assert_eq!(updated.pts(), original.pts());

        // Test bits per sample patching
        let mut test_buffer = buffer.clone();
        FrameHeader::patch_bits_per_sample(&mut test_buffer, 32).unwrap();
        let updated = FrameHeader::decode(&mut &test_buffer[..]).unwrap();
        assert_eq!(updated.bits_per_sample(), 32); // Changed field
        assert_eq!(*updated.encoding(), *original.encoding());
        assert_eq!(updated.sample_size(), original.sample_size());
        assert_eq!(updated.sample_rate(), original.sample_rate());
        assert_eq!(updated.channels(), original.channels());
        assert_eq!(*updated.endianness(), *original.endianness());
        assert_eq!(updated.id(), original.id());
        assert_eq!(updated.pts(), original.pts());

        // Test channels patching
        let mut test_buffer = buffer.clone();
        FrameHeader::patch_channels(&mut test_buffer, 8).unwrap();
        let updated = FrameHeader::decode(&mut &test_buffer[..]).unwrap();
        assert_eq!(updated.channels(), 8); // Changed field
        assert_eq!(*updated.encoding(), *original.encoding());
        assert_eq!(updated.sample_size(), original.sample_size());
        assert_eq!(updated.sample_rate(), original.sample_rate());
        assert_eq!(updated.bits_per_sample(), original.bits_per_sample());
        assert_eq!(*updated.endianness(), *original.endianness());
        assert_eq!(updated.id(), original.id());
        assert_eq!(updated.pts(), original.pts());

        // Test ID patching
        let mut test_buffer = buffer.clone();
        FrameHeader::patch_id(&mut test_buffer, Some(0xFEEDFACE)).unwrap();
        let updated = FrameHeader::decode(&mut &test_buffer[..]).unwrap();
        assert_eq!(updated.id(), Some(0xFEEDFACE)); // Changed field
        assert_eq!(*updated.encoding(), *original.encoding());
        assert_eq!(updated.sample_size(), original.sample_size());
        assert_eq!(updated.sample_rate(), original.sample_rate());
        assert_eq!(updated.channels(), original.channels());
        assert_eq!(updated.bits_per_sample(), original.bits_per_sample());
        assert_eq!(*updated.endianness(), *original.endianness());
        assert_eq!(updated.pts(), original.pts());

        // Test PTS patching
        let mut test_buffer = buffer.clone();
        FrameHeader::patch_pts(&mut test_buffer, Some(0xF00DFACE)).unwrap();
        let updated = FrameHeader::decode(&mut &test_buffer[..]).unwrap();
        assert_eq!(updated.pts(), Some(0xF00DFACE)); // Changed field
        assert_eq!(*updated.encoding(), *original.encoding());
        assert_eq!(updated.sample_size(), original.sample_size());
        assert_eq!(updated.sample_rate(), original.sample_rate());
        assert_eq!(updated.channels(), original.channels());
        assert_eq!(updated.bits_per_sample(), original.bits_per_sample());
        assert_eq!(*updated.endianness(), *original.endianness());
        assert_eq!(updated.id(), original.id());
    }

    #[test]
    fn test_magic_word_off_by_one() {
        // Create a valid header as base
        let valid_header = FrameHeader::new(
            EncodingFlag::PCMSigned,
            1024,
            48000,
            2,
            24,
            Endianness::LittleEndian,
            None,
            None,
        )
        .unwrap();
        let mut valid_buffer = Vec::new();
        valid_header.encode(&mut valid_buffer).unwrap();

        // Test magic word off by one higher
        let mut buffer = valid_buffer.clone();
        let mut header = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        header = (header & !FrameHeader::MAGIC_MASK)
            | ((FrameHeader::MAGIC_WORD + 1) << FrameHeader::MAGIC_SHIFT);
        buffer[..4].copy_from_slice(&header.to_be_bytes());
        assert!(
            FrameHeader::decode(&mut &buffer[..]).is_err(),
            "Failed to detect magic word too high"
        );
        assert!(
            !FrameHeader::validate_header(&buffer).unwrap(),
            "Validation passed with magic word too high"
        );

        // Test magic word off by one lower
        let mut buffer = valid_buffer.clone();
        let mut header = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        header = (header & !FrameHeader::MAGIC_MASK)
            | ((FrameHeader::MAGIC_WORD - 1) << FrameHeader::MAGIC_SHIFT);
        buffer[..4].copy_from_slice(&header.to_be_bytes());
        assert!(
            FrameHeader::decode(&mut &buffer[..]).is_err(),
            "Failed to detect magic word too low"
        );
        assert!(
            !FrameHeader::validate_header(&buffer).unwrap(),
            "Validation passed with magic word too low"
        );

        // Test magic word shifted right by one bit
        let mut buffer = valid_buffer.clone();
        let mut header = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        header = (header & !FrameHeader::MAGIC_MASK)
            | ((FrameHeader::MAGIC_WORD >> 1) << FrameHeader::MAGIC_SHIFT);
        buffer[..4].copy_from_slice(&header.to_be_bytes());
        assert!(
            FrameHeader::decode(&mut &buffer[..]).is_err(),
            "Failed to detect magic word bit-shifted right"
        );
        assert!(
            !FrameHeader::validate_header(&buffer).unwrap(),
            "Validation passed with magic word bit-shifted right"
        );

        // Test magic word shifted left by one bit
        let mut buffer = valid_buffer.clone();
        let mut header = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        header = (header & !FrameHeader::MAGIC_MASK)
            | ((FrameHeader::MAGIC_WORD << 1) << FrameHeader::MAGIC_SHIFT);
        buffer[..4].copy_from_slice(&header.to_be_bytes());
        assert!(
            FrameHeader::decode(&mut &buffer[..]).is_err(),
            "Failed to detect magic word bit-shifted left"
        );
        assert!(
            !FrameHeader::validate_header(&buffer).unwrap(),
            "Validation passed with magic word bit-shifted left"
        );

        // Test magic word at wrong bit position (shifted by 1 in final header)
        let mut buffer = valid_buffer;
        let mut header = u32::from_be_bytes(buffer[..4].try_into().unwrap());
        header = (header & !FrameHeader::MAGIC_MASK)
            | (FrameHeader::MAGIC_WORD << (FrameHeader::MAGIC_SHIFT + 1));
        buffer[..4].copy_from_slice(&header.to_be_bytes());
        assert!(
            FrameHeader::decode(&mut &buffer[..]).is_err(),
            "Failed to detect magic word at wrong position"
        );
        assert!(
            !FrameHeader::validate_header(&buffer).unwrap(),
            "Validation passed with magic word at wrong position"
        );
    }
}

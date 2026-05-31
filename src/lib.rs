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

fn encoding_from_code(code: u32) -> Option<EncodingFlag> {
    match code {
        0 => Some(EncodingFlag::PCMSigned),
        1 => Some(EncodingFlag::PCMFloat),
        2 => Some(EncodingFlag::Opus),
        3 => Some(EncodingFlag::FLAC),
        4 => Some(EncodingFlag::AAC),
        5 => Some(EncodingFlag::H264),
        _ => None,
    }
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

const fn make_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;

    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                0xEDB8_8320u32 ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }

    table
}

const CRC32_TABLE: [u32; 256] = make_crc32_table();

pub fn crc32_ieee(bytes: &[u8]) -> u32 {
    crc32_ieee_update(0, bytes)
}

pub fn crc32_ieee_update(previous: u32, bytes: &[u8]) -> u32 {
    let mut crc = !previous;

    for &byte in bytes {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC32_TABLE[index] ^ (crc >> 8);
    }

    !crc
}

pub fn packet_crc32(header_without_crc: &[u8], payload: &[u8]) -> u32 {
    crc32_ieee_update(crc32_ieee(header_without_crc), payload)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FrameHeaderV2 {
    encoding: EncodingFlag,
    payload_size: u32,
    frame_count: u32,
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
    endianness: Endianness,
    id: Option<u64>,
    id_is_u64: bool,
    pts: Option<u64>,
    packet_crc32: Option<u32>,
    packet_flags: u8,
}

impl FrameHeaderV2 {
    const MAGIC_WORD: u32 = 0x2B;
    const VERSION: u32 = 2;

    const MAGIC_SHIFT: u32 = 26;
    const MAGIC_MASK: u32 = 0x3F << 26;

    const VERSION_SHIFT: u32 = 24;
    const VERSION_MASK: u32 = 0x3 << 24;

    const FLAGS_SHIFT: u32 = 16;
    const FLAGS_MASK: u32 = 0xFF << 16;

    const ENCODING_SHIFT: u32 = 12;
    const ENCODING_MASK: u32 = 0xF << 12;

    const SAMPLE_RATE_SHIFT: u32 = 8;
    const SAMPLE_RATE_MASK: u32 = 0xF << 8;

    const CHANNELS_SHIFT: u32 = 3;
    const CHANNELS_MASK: u32 = 0x1F << 3;

    const BITS_MASK: u32 = 0x7;

    const FLAG_ID_PRESENT: u8 = 1 << 0;
    const FLAG_ID_U64: u8 = 1 << 1;
    const FLAG_PTS_PRESENT: u8 = 1 << 2;
    const FLAG_PACKET_CRC32_PRESENT: u8 = 1 << 3;
    const FLAG_BIG_ENDIAN: u8 = 1 << 4;
    const FLAG_EXTENDED_SIZES: u8 = 1 << 5;
    pub const FLAG_DISCONTINUITY: u8 = 1 << 6;
    pub const FLAG_CONFIG: u8 = 1 << 7;
    const PUBLIC_PACKET_FLAGS: u8 = Self::FLAG_DISCONTINUITY | Self::FLAG_CONFIG;

    pub const BASE_SIZE: usize = 8;
    pub const EXTENDED_SIZE_BYTES: usize = 8;
    pub const SHORT_SIZE_MAX: u32 = 0xFFFE;
    const SHORT_SIZE_SENTINEL: u32 = 0xFFFF;
    pub const VALID_SAMPLE_RATES: [u32; 11] = [
        8000, 12000, 16000, 24000, 32000, 44100, 48000, 88200, 96000, 176400, 192000,
    ];

    pub fn new(
        encoding: EncodingFlag,
        payload_size: u32,
        frame_count: u32,
        sample_rate: u32,
        channels: u8,
        bits_per_sample: u8,
        endianness: Endianness,
        id: Option<u64>,
        pts: Option<u64>,
        packet_crc32: Option<u32>,
    ) -> Result<Self, String> {
        let header = FrameHeaderV2 {
            encoding,
            payload_size,
            frame_count,
            sample_rate,
            channels,
            bits_per_sample,
            endianness,
            id,
            id_is_u64: id.map(|value| value > u32::MAX as u64).unwrap_or(false),
            pts,
            packet_crc32,
            packet_flags: 0,
        };
        header.validate_fields()?;
        Ok(header)
    }

    pub fn with_packet_flags(mut self, packet_flags: u8) -> Result<Self, String> {
        self.packet_flags = packet_flags;
        self.validate_fields()?;
        Ok(self)
    }

    pub fn with_packet_crc32(mut self, payload: &[u8]) -> Result<Self, String> {
        let crc = self.compute_packet_crc32(payload)?;
        self.packet_crc32 = Some(crc);
        Ok(self)
    }

    pub fn compute_packet_crc32(&self, payload: &[u8]) -> Result<u32, String> {
        let mut header_with_crc_flag = self.clone();
        header_with_crc_flag.packet_crc32 = Some(0);

        let mut bytes = Vec::with_capacity(header_with_crc_flag.size());
        header_with_crc_flag
            .encode(&mut bytes)
            .map_err(|err| err.to_string())?;
        Ok(packet_crc32(&bytes[..bytes.len() - 4], payload))
    }

    pub fn verify_packet_crc32(
        &self,
        encoded_header: &[u8],
        payload: &[u8],
    ) -> Result<bool, String> {
        let expected = match self.packet_crc32 {
            Some(value) => value,
            None => return Ok(false),
        };
        let header_size = self.size();
        if encoded_header.len() < header_size {
            return Err("Encoded header too small".to_string());
        }

        Ok(packet_crc32(&encoded_header[..header_size - 4], payload) == expected)
    }

    pub fn encode<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.validate_fields()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

        let sample_rate_code = Self::sample_rate_code(self.sample_rate)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid sample rate"))?;
        let bits_code = Self::bits_code(self.bits_per_sample).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid bits per sample")
        })?;
        let flags = self.encoded_flags();
        let extended_sizes = self.needs_extended_sizes();

        let mut word: u32 = Self::MAGIC_WORD << Self::MAGIC_SHIFT;
        word |= Self::VERSION << Self::VERSION_SHIFT;
        word |= (flags as u32) << Self::FLAGS_SHIFT;
        word |= (self.encoding as u32) << Self::ENCODING_SHIFT;
        word |= sample_rate_code << Self::SAMPLE_RATE_SHIFT;
        word |= ((self.channels - 1) as u32) << Self::CHANNELS_SHIFT;
        word |= bits_code;

        writer.write_all(&word.to_be_bytes())?;
        let size_word = if extended_sizes {
            (Self::SHORT_SIZE_SENTINEL << 16) | Self::SHORT_SIZE_SENTINEL
        } else {
            (self.payload_size << 16) | self.frame_count
        };
        writer.write_all(&size_word.to_be_bytes())?;

        if extended_sizes {
            writer.write_all(&self.payload_size.to_be_bytes())?;
            writer.write_all(&self.frame_count.to_be_bytes())?;
        }

        if let Some(id) = self.id {
            if self.encoded_id_is_u64() {
                writer.write_all(&id.to_be_bytes())?;
            } else {
                writer.write_all(&(id as u32).to_be_bytes())?;
            }
        }
        if let Some(pts) = self.pts {
            writer.write_all(&pts.to_be_bytes())?;
        }
        if let Some(crc) = self.packet_crc32 {
            writer.write_all(&crc.to_be_bytes())?;
        }

        Ok(())
    }

    pub fn decode<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut base = [0u8; Self::BASE_SIZE];
        reader.read_exact(&mut base)?;

        let word = u32::from_be_bytes(base[..4].try_into().unwrap());
        if (word & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid v2 header magic word",
            ));
        }
        if (word & Self::VERSION_MASK) >> Self::VERSION_SHIFT != Self::VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid v2 header version",
            ));
        }

        let flags = ((word & Self::FLAGS_MASK) >> Self::FLAGS_SHIFT) as u8;
        if flags & Self::FLAG_ID_U64 != 0 && flags & Self::FLAG_ID_PRESENT == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "v2 header has 64-bit ID flag without ID present",
            ));
        }

        let encoding_code = (word & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT;
        let encoding = encoding_from_code(encoding_code).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Invalid v2 encoding flag")
        })?;

        let sample_rate_code = (word & Self::SAMPLE_RATE_MASK) >> Self::SAMPLE_RATE_SHIFT;
        let sample_rate = Self::sample_rate_from_code(sample_rate_code).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Invalid v2 sample rate code")
        })?;

        let bits_code = word & Self::BITS_MASK;
        let bits_per_sample = Self::bits_from_code(bits_code).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid v2 bits-per-sample code",
            )
        })?;

        let channels = (((word & Self::CHANNELS_MASK) >> Self::CHANNELS_SHIFT) + 1) as u8;
        let size_word = u32::from_be_bytes(base[4..8].try_into().unwrap());
        let extended_sizes = flags & Self::FLAG_EXTENDED_SIZES != 0;
        if extended_sizes && size_word != 0xFFFF_FFFF {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extended v2 sizes must use short-size sentinels",
            ));
        }
        if !extended_sizes
            && (((size_word >> 16) & 0xFFFF) == Self::SHORT_SIZE_SENTINEL
                || (size_word & 0xFFFF) == Self::SHORT_SIZE_SENTINEL)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "v2 short-size sentinel requires extended sizes",
            ));
        }
        let (payload_size, frame_count) = if extended_sizes {
            let mut sizes = [0u8; Self::EXTENDED_SIZE_BYTES];
            reader.read_exact(&mut sizes)?;
            (
                u32::from_be_bytes(sizes[..4].try_into().unwrap()),
                u32::from_be_bytes(sizes[4..8].try_into().unwrap()),
            )
        } else {
            ((size_word >> 16) & 0xFFFF, size_word & 0xFFFF)
        };

        let id_is_u64 = flags & Self::FLAG_ID_U64 != 0;
        let id = if flags & Self::FLAG_ID_PRESENT != 0 {
            if id_is_u64 {
                let mut id_bytes = [0u8; 8];
                reader.read_exact(&mut id_bytes)?;
                Some(u64::from_be_bytes(id_bytes))
            } else {
                let mut id_bytes = [0u8; 4];
                reader.read_exact(&mut id_bytes)?;
                Some(u32::from_be_bytes(id_bytes) as u64)
            }
        } else {
            None
        };

        let pts = if flags & Self::FLAG_PTS_PRESENT != 0 {
            let mut pts_bytes = [0u8; 8];
            reader.read_exact(&mut pts_bytes)?;
            Some(u64::from_be_bytes(pts_bytes))
        } else {
            None
        };

        let packet_crc32 = if flags & Self::FLAG_PACKET_CRC32_PRESENT != 0 {
            let mut crc_bytes = [0u8; 4];
            reader.read_exact(&mut crc_bytes)?;
            Some(u32::from_be_bytes(crc_bytes))
        } else {
            None
        };

        let header = FrameHeaderV2 {
            encoding,
            payload_size,
            frame_count,
            sample_rate,
            channels,
            bits_per_sample,
            endianness: if flags & Self::FLAG_BIG_ENDIAN != 0 {
                Endianness::BigEndian
            } else {
                Endianness::LittleEndian
            },
            id,
            id_is_u64,
            pts,
            packet_crc32,
            packet_flags: flags & Self::PUBLIC_PACKET_FLAGS,
        };
        header
            .validate_fields()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        Ok(header)
    }

    pub fn validate_header(header_bytes: &[u8]) -> Result<bool, String> {
        if header_bytes.len() < Self::BASE_SIZE {
            return Err("Header too small".to_string());
        }

        let word = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        if (word & Self::MAGIC_MASK) >> Self::MAGIC_SHIFT != Self::MAGIC_WORD {
            return Ok(false);
        }
        if (word & Self::VERSION_MASK) >> Self::VERSION_SHIFT != Self::VERSION {
            return Ok(false);
        }

        let flags = ((word & Self::FLAGS_MASK) >> Self::FLAGS_SHIFT) as u8;
        if flags & Self::FLAG_ID_U64 != 0 && flags & Self::FLAG_ID_PRESENT == 0 {
            return Ok(false);
        }
        if encoding_from_code((word & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT).is_none() {
            return Ok(false);
        }
        if Self::sample_rate_from_code((word & Self::SAMPLE_RATE_MASK) >> Self::SAMPLE_RATE_SHIFT)
            .is_none()
        {
            return Ok(false);
        }
        if Self::bits_from_code(word & Self::BITS_MASK).is_none() {
            return Ok(false);
        }
        let size_word = u32::from_be_bytes(header_bytes[4..8].try_into().unwrap());
        let extended_sizes = flags & Self::FLAG_EXTENDED_SIZES != 0;
        if extended_sizes && size_word != 0xFFFF_FFFF {
            return Ok(false);
        }
        if !extended_sizes
            && (((size_word >> 16) & 0xFFFF) == Self::SHORT_SIZE_SENTINEL
                || (size_word & 0xFFFF) == Self::SHORT_SIZE_SENTINEL)
        {
            return Ok(false);
        }

        let expected_size = Self::BASE_SIZE
            + if extended_sizes {
                Self::EXTENDED_SIZE_BYTES
            } else {
                0
            }
            + if flags & Self::FLAG_ID_PRESENT != 0 {
                if flags & Self::FLAG_ID_U64 != 0 {
                    8
                } else {
                    4
                }
            } else {
                0
            }
            + if flags & Self::FLAG_PTS_PRESENT != 0 {
                8
            } else {
                0
            }
            + if flags & Self::FLAG_PACKET_CRC32_PRESENT != 0 {
                4
            } else {
                0
            };

        Ok(header_bytes.len() >= expected_size)
    }

    pub fn size(&self) -> usize {
        Self::BASE_SIZE
            + if self.needs_extended_sizes() {
                Self::EXTENDED_SIZE_BYTES
            } else {
                0
            }
            + self.encoded_id_bytes()
            + (self.pts.is_some() as usize) * 8
            + (self.packet_crc32.is_some() as usize) * 4
    }

    pub fn encoding(&self) -> &EncodingFlag {
        &self.encoding
    }

    pub fn payload_size(&self) -> u32 {
        self.payload_size
    }

    pub fn frame_count(&self) -> u32 {
        self.frame_count
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

    pub fn id_is_u64(&self) -> bool {
        self.id.is_some() && self.encoded_id_is_u64()
    }

    pub fn pts(&self) -> Option<u64> {
        self.pts
    }

    pub fn packet_crc32_value(&self) -> Option<u32> {
        self.packet_crc32
    }

    pub fn packet_flags(&self) -> u8 {
        self.packet_flags
    }

    pub fn extract_payload_size(header_bytes: &[u8]) -> Result<u32, String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid v2 header".to_string());
        }
        let word = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        let flags = ((word & Self::FLAGS_MASK) >> Self::FLAGS_SHIFT) as u8;
        if flags & Self::FLAG_EXTENDED_SIZES != 0 {
            Ok(u32::from_be_bytes(header_bytes[8..12].try_into().unwrap()))
        } else {
            let sizes = u32::from_be_bytes(header_bytes[4..8].try_into().unwrap());
            Ok((sizes >> 16) & 0xFFFF)
        }
    }

    pub fn extract_frame_count(header_bytes: &[u8]) -> Result<u32, String> {
        if !Self::validate_header(header_bytes)? {
            return Err("Invalid v2 header".to_string());
        }
        let word = u32::from_be_bytes(header_bytes[..4].try_into().unwrap());
        let flags = ((word & Self::FLAGS_MASK) >> Self::FLAGS_SHIFT) as u8;
        if flags & Self::FLAG_EXTENDED_SIZES != 0 {
            Ok(u32::from_be_bytes(header_bytes[12..16].try_into().unwrap()))
        } else {
            let sizes = u32::from_be_bytes(header_bytes[4..8].try_into().unwrap());
            Ok(sizes & 0xFFFF)
        }
    }

    fn validate_fields(&self) -> Result<(), String> {
        if self.channels == 0 || self.channels > 32 {
            return Err("Channel count must be between 1 and 32".to_string());
        }
        if Self::sample_rate_code(self.sample_rate).is_none() {
            return Err(format!(
                "Invalid sample rate: {}. Must be one of: {:?}",
                self.sample_rate,
                Self::VALID_SAMPLE_RATES
            ));
        }
        if Self::bits_code(self.bits_per_sample).is_none() {
            return Err("Bits per sample must be 0, 8, 16, 24, 32, or 64".to_string());
        }
        if matches!(
            self.encoding,
            EncodingFlag::PCMSigned | EncodingFlag::PCMFloat
        ) && self.bits_per_sample == 0
        {
            return Err("PCM headers must set bits per sample".to_string());
        }
        if self.packet_flags & !Self::PUBLIC_PACKET_FLAGS != 0 {
            return Err("Unsupported v2 packet flags set".to_string());
        }
        if self.id.is_none() && self.id_is_u64 {
            return Err("64-bit ID flag requires an ID".to_string());
        }
        if !self.needs_extended_sizes()
            && (self.payload_size == Self::SHORT_SIZE_SENTINEL
                || self.frame_count == Self::SHORT_SIZE_SENTINEL)
        {
            return Err("Short v2 size fields reserve 65535 as the extension sentinel".to_string());
        }
        Ok(())
    }

    fn encoded_flags(&self) -> u8 {
        let mut flags = self.packet_flags & Self::PUBLIC_PACKET_FLAGS;
        if self.id.is_some() {
            flags |= Self::FLAG_ID_PRESENT;
        }
        if self.encoded_id_is_u64() {
            flags |= Self::FLAG_ID_U64;
        }
        if self.pts.is_some() {
            flags |= Self::FLAG_PTS_PRESENT;
        }
        if self.packet_crc32.is_some() {
            flags |= Self::FLAG_PACKET_CRC32_PRESENT;
        }
        if self.endianness == Endianness::BigEndian {
            flags |= Self::FLAG_BIG_ENDIAN;
        }
        if self.needs_extended_sizes() {
            flags |= Self::FLAG_EXTENDED_SIZES;
        }
        flags
    }

    fn needs_extended_sizes(&self) -> bool {
        self.payload_size > Self::SHORT_SIZE_MAX || self.frame_count > Self::SHORT_SIZE_MAX
    }

    fn encoded_id_is_u64(&self) -> bool {
        self.id
            .map(|id| self.id_is_u64 || id > u32::MAX as u64)
            .unwrap_or(false)
    }

    fn encoded_id_bytes(&self) -> usize {
        if self.id.is_none() {
            0
        } else if self.encoded_id_is_u64() {
            8
        } else {
            4
        }
    }

    fn sample_rate_code(sample_rate: u32) -> Option<u32> {
        Self::VALID_SAMPLE_RATES
            .iter()
            .position(|&rate| rate == sample_rate)
            .map(|index| index as u32)
    }

    fn sample_rate_from_code(code: u32) -> Option<u32> {
        Self::VALID_SAMPLE_RATES.get(code as usize).copied()
    }

    fn bits_code(bits_per_sample: u8) -> Option<u32> {
        match bits_per_sample {
            0 => Some(0),
            8 => Some(1),
            16 => Some(2),
            24 => Some(3),
            32 => Some(4),
            64 => Some(5),
            _ => None,
        }
    }

    fn bits_from_code(code: u32) -> Option<u8> {
        match code {
            0 => Some(0),
            1 => Some(8),
            2 => Some(16),
            3 => Some(24),
            4 => Some(32),
            5 => Some(64),
            _ => None,
        }
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

        let encoding =
            match encoding_from_code((header & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT) {
                Some(encoding) => encoding,
                None => {
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
        if encoding > 5 {
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

        encoding_from_code((header & Self::ENCODING_MASK) >> Self::ENCODING_SHIFT)
            .ok_or_else(|| "Invalid encoding flag".to_string())
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
        let header_bytes = create_test_header();
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

    #[test]
    fn test_crc32_ieee_known_vector() {
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn test_v2_compact_opus_roundtrip() {
        let header = FrameHeaderV2::new(
            EncodingFlag::Opus,
            127,
            960,
            48000,
            2,
            0,
            Endianness::LittleEndian,
            Some(7),
            Some(48_000),
            None,
        )
        .unwrap();

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();

        assert_eq!(buffer.len(), 20);
        assert!(FrameHeaderV2::validate_header(&buffer).unwrap());
        assert_eq!(FrameHeaderV2::extract_payload_size(&buffer).unwrap(), 127);
        assert_eq!(FrameHeaderV2::extract_frame_count(&buffer).unwrap(), 960);

        let decoded = FrameHeaderV2::decode(&mut &buffer[..]).unwrap();
        assert_eq!(*decoded.encoding(), EncodingFlag::Opus);
        assert_eq!(decoded.payload_size(), 127);
        assert_eq!(decoded.frame_count(), 960);
        assert_eq!(decoded.sample_rate(), 48000);
        assert_eq!(decoded.channels(), 2);
        assert_eq!(decoded.bits_per_sample(), 0);
        assert_eq!(*decoded.endianness(), Endianness::LittleEndian);
        assert_eq!(decoded.id(), Some(7));
        assert!(!decoded.id_is_u64());
        assert_eq!(decoded.pts(), Some(48_000));
        assert_eq!(decoded.size(), 20);
    }

    #[test]
    fn test_v2_packet_crc32() {
        let payload = [0x11, 0x22, 0x33, 0x44];
        let header = FrameHeaderV2::new(
            EncodingFlag::Opus,
            payload.len() as u32,
            960,
            48000,
            2,
            0,
            Endianness::LittleEndian,
            Some(9),
            Some(960),
            None,
        )
        .unwrap()
        .with_packet_crc32(&payload)
        .unwrap();

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();

        assert_eq!(buffer.len(), 24);
        let decoded = FrameHeaderV2::decode(&mut &buffer[..]).unwrap();
        assert!(decoded.verify_packet_crc32(&buffer, &payload).unwrap());

        let mut corrupted_payload = payload;
        corrupted_payload[0] ^= 0xFF;
        assert!(!decoded
            .verify_packet_crc32(&buffer, &corrupted_payload)
            .unwrap());
    }

    #[test]
    fn test_v2_extended_sizes_roundtrip() {
        let header = FrameHeaderV2::new(
            EncodingFlag::FLAC,
            70_000,
            70_001,
            96000,
            2,
            24,
            Endianness::BigEndian,
            None,
            None,
            None,
        )
        .unwrap();

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();

        assert_eq!(
            buffer.len(),
            FrameHeaderV2::BASE_SIZE + FrameHeaderV2::EXTENDED_SIZE_BYTES
        );
        assert!(FrameHeaderV2::validate_header(&buffer).unwrap());
        assert_eq!(
            FrameHeaderV2::extract_payload_size(&buffer).unwrap(),
            70_000
        );
        assert_eq!(FrameHeaderV2::extract_frame_count(&buffer).unwrap(), 70_001);

        let decoded = FrameHeaderV2::decode(&mut &buffer[..]).unwrap();
        assert_eq!(decoded.payload_size(), 70_000);
        assert_eq!(decoded.frame_count(), 70_001);
        assert_eq!(*decoded.endianness(), Endianness::BigEndian);
    }

    #[test]
    fn test_v2_uses_u64_id_only_when_needed() {
        let header = FrameHeaderV2::new(
            EncodingFlag::AAC,
            512,
            1024,
            48000,
            2,
            0,
            Endianness::LittleEndian,
            Some(u32::MAX as u64 + 1),
            None,
            None,
        )
        .unwrap();

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();

        assert_eq!(buffer.len(), FrameHeaderV2::BASE_SIZE + 8);
        let decoded = FrameHeaderV2::decode(&mut &buffer[..]).unwrap();
        assert_eq!(decoded.id(), Some(u32::MAX as u64 + 1));
        assert!(decoded.id_is_u64());
    }

    #[test]
    fn test_v2_rejects_short_size_sentinel_without_extension() {
        let mut buffer = Vec::new();
        let header = FrameHeaderV2::new(
            EncodingFlag::Opus,
            1,
            960,
            48000,
            2,
            0,
            Endianness::LittleEndian,
            None,
            None,
            None,
        )
        .unwrap();
        header.encode(&mut buffer).unwrap();

        let size_word = (FrameHeaderV2::SHORT_SIZE_SENTINEL << 16) | 960;
        buffer[4..8].copy_from_slice(&size_word.to_be_bytes());

        assert!(!FrameHeaderV2::validate_header(&buffer).unwrap());
        assert!(FrameHeaderV2::decode(&mut &buffer[..]).is_err());
    }
}

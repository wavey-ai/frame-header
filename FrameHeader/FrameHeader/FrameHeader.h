#import <Foundation/Foundation.h>

typedef NS_ENUM(NSUInteger, Endianness) {
    EndiannessLittle,
    EndiannessBig,
};

typedef NS_ENUM(NSUInteger, EncodingFlag) {
    EncodingFlagPCMSigned = 0,
    EncodingFlagPCMFloat = 1,
    EncodingFlagOpus = 2,
    EncodingFlagFLAC = 3,
    EncodingFlagAAC = 4,
    EncodingFlagH264 = 5,
};

NS_ASSUME_NONNULL_BEGIN

@interface FrameHeader : NSObject

@property(nonatomic, assign) EncodingFlag encoding;
@property(nonatomic, assign) uint16_t sampleSize;
@property(nonatomic, assign) uint32_t sampleRate;
@property(nonatomic, assign) uint8_t channels;
@property(nonatomic, assign) uint8_t bitsPerSample;
@property(nonatomic, assign) Endianness endianness;
/// Optional 64‑bit frame ID (nil if not set)
@property(nonatomic, strong, nullable) NSNumber *frameID;
/// Optional 64‑bit presentation timestamp (nil if not set)
@property(nonatomic, strong, nullable) NSNumber *pts;

/// Initializes a new header. Returns nil (and an error) if parameters are
/// invalid. Validations: • channels must be 1..16 • bitsPerSample must be 16,
/// 24, or 32 • sampleSize must not exceed 0xFFF • sampleRate must be one of:
/// 44100, 48000, 88200, or 96000.
- (nullable instancetype)initWithEncoding:(EncodingFlag)encoding
                               sampleSize:(uint16_t)sampleSize
                               sampleRate:(uint32_t)sampleRate
                                 channels:(uint8_t)channels
                            bitsPerSample:(uint8_t)bitsPerSample
                               endianness:(Endianness)endianness
                                  frameID:(nullable NSNumber *)frameID
                                      pts:(nullable NSNumber *)pts
                                    error:(NSError **)error;

/// Encodes the header into an NSData object. The layout is:
/// • 4 bytes: the packed header (big‑endian)
/// • 8 bytes: frameID (if set)
/// • 8 bytes: pts (if set)
- (NSData *)encodeWithError:(NSError **)error;

/// Decodes a header from NSData. Returns nil (and an error) on failure.
+ (nullable instancetype)decodeFromData:(NSData *)data error:(NSError **)error;

/// Returns the total header size (4 bytes plus optional 8‑byte fields for
/// frameID and/or pts).
- (NSUInteger)size;

#pragma mark - Extraction Methods

/// Checks that the first 4 bytes contain a valid header.
+ (BOOL)validateHeaderData:(NSData *)data error:(NSError **)error;
/// Returns the sampleSize field from the header data.
+ (uint16_t)extractSampleSizeFromData:(NSData *)data error:(NSError **)error;
/// Returns the encoding flag from the header data.
+ (EncodingFlag)extractEncodingFromData:(NSData *)data error:(NSError **)error;
/// Returns the frameID from the header data (or nil if not present).
+ (nullable NSNumber *)extractFrameIDFromData:(NSData *)data
                                        error:(NSError **)error;
/// Returns the pts value from the header data (or nil if not present).
+ (nullable NSNumber *)extractPTSFromData:(NSData *)data
                                    error:(NSError **)error;

#pragma mark - Patch Methods

/// Updates the bits-per‑sample field in the mutable header data.
+ (BOOL)patchBitsPerSampleInData:(NSMutableData *)data
                            bits:(uint8_t)bits
                           error:(NSError **)error;
/// Updates the sampleSize field in the mutable header data.
+ (BOOL)patchSampleSizeInData:(NSMutableData *)data
                newSampleSize:(uint16_t)newSampleSize
                        error:(NSError **)error;
/// Updates the encoding flag in the mutable header data.
+ (BOOL)patchEncodingInData:(NSMutableData *)data
                   encoding:(EncodingFlag)encoding
                      error:(NSError **)error;
/// Updates the sample rate in the mutable header data.
+ (BOOL)patchSampleRateInData:(NSMutableData *)data
                   sampleRate:(uint32_t)sampleRate
                        error:(NSError **)error;
/// Updates the channel count in the mutable header data.
+ (BOOL)patchChannelsInData:(NSMutableData *)data
                   channels:(uint8_t)channels
                      error:(NSError **)error;
/// Updates (or clears) the frameID in the mutable header data.
+ (BOOL)patchFrameIDInData:(NSMutableData *)data
                   frameID:(nullable NSNumber *)frameID
                     error:(NSError **)error;
/// Updates (or clears) the pts value in the mutable header data.
+ (BOOL)patchPTSInData:(NSMutableData *)data
                   pts:(nullable NSNumber *)pts
                 error:(NSError **)error;

@end

NS_ASSUME_NONNULL_END

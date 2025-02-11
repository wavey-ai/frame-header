#import "FrameHeader.h"
#import <CoreFoundation/CoreFoundation.h>
#import <string.h>

// These constants define the bit–layout; they match the Rust version.
static const uint32_t kMagicWord = 0x2A;
static const uint32_t kMagicShift = 26;
static const uint32_t kMagicMask = 0x3F << kMagicShift;

static const uint32_t kSampleRateShift = 24;
static const uint32_t kSampleRateMask = 0x3 << kSampleRateShift;

static const uint32_t kBitsShift = 22;
static const uint32_t kBitsMask = 0x3 << kBitsShift;

static const uint32_t kPTSShift = 21;
static const uint32_t kPTSMask = 0x1 << kPTSShift;

static const uint32_t kIDShift = 20;
static const uint32_t kIDMask = 0x1 << kIDShift;

static const uint32_t kEncodingShift = 17;
static const uint32_t kEncodingMask = 0x7 << kEncodingShift;

static const uint32_t kEndianShift = 16;
static const uint32_t kEndianMask = 0x1 << kEndianShift;

static const uint32_t kChannelsShift = 12;
static const uint32_t kChannelsMask = 0xF << kChannelsShift;

static const uint32_t kSampleSizeMask = 0xFFF;

static const uint32_t validSampleRates[] = {44100, 48000, 88200, 96000};
static const size_t validSampleRatesCount = 4;
static const uint16_t kMaxSampleSize = 0xFFF;

@implementation FrameHeader

#pragma mark - Initializer

- (nullable instancetype)initWithEncoding:(EncodingFlag)encoding
                               sampleSize:(uint16_t)sampleSize
                               sampleRate:(uint32_t)sampleRate
                                 channels:(uint8_t)channels
                            bitsPerSample:(uint8_t)bitsPerSample
                               endianness:(Endianness)endianness
                                  frameID:(nullable NSNumber *)frameID
                                      pts:(nullable NSNumber *)pts
                                    error:(NSError **)error {
    self = [super init];
    if (self) {
        if (channels == 0 || channels > 16) {
            if (error) {
                *error = [NSError
                    errorWithDomain:@"FrameHeaderErrorDomain"
                               code:100
                           userInfo:@{
                               NSLocalizedDescriptionKey :
                                   @"Channel count must be between 1 and 16"
                           }];
            }
            return nil;
        }
        if (!(bitsPerSample == 16 || bitsPerSample == 24 ||
              bitsPerSample == 32)) {
            if (error) {
                *error = [NSError
                    errorWithDomain:@"FrameHeaderErrorDomain"
                               code:101
                           userInfo:@{
                               NSLocalizedDescriptionKey :
                                   @"Bits per sample must be 16, 24, or 32"
                           }];
            }
            return nil;
        }
        if (sampleSize > kMaxSampleSize) {
            if (error) {
                NSString *msg = [NSString
                    stringWithFormat:@"Sample size exceeds maximum value (%u)",
                                     kMaxSampleSize];
                *error = [NSError
                    errorWithDomain:@"FrameHeaderErrorDomain"
                               code:102
                           userInfo:@{NSLocalizedDescriptionKey : msg}];
            }
            return nil;
        }
        BOOL validRate = NO;
        for (size_t i = 0; i < validSampleRatesCount; i++) {
            if (sampleRate == validSampleRates[i]) {
                validRate = YES;
                break;
            }
        }
        if (!validRate) {
            if (error) {
                NSString *msg = [NSString
                    stringWithFormat:@"Invalid sample rate: %u. Must be one "
                                     @"of: [44100, 48000, 88200, 96000]",
                                     sampleRate];
                *error = [NSError
                    errorWithDomain:@"FrameHeaderErrorDomain"
                               code:103
                           userInfo:@{NSLocalizedDescriptionKey : msg}];
            }
            return nil;
        }
        _encoding = encoding;
        _sampleSize = sampleSize;
        _sampleRate = sampleRate;
        _channels = channels;
        _bitsPerSample = bitsPerSample;
        _endianness = endianness;
        _frameID = frameID;
        _pts = pts;
    }
    return self;
}

#pragma mark - Encoding

- (NSData *)encodeWithError:(NSError **)error {
    uint32_t header = 0;
    header |= (kMagicWord << kMagicShift);

    uint32_t sampleRateCode = 0;
    if (self.sampleRate == 44100) {
        sampleRateCode = 0;
    } else if (self.sampleRate == 48000) {
        sampleRateCode = 1;
    } else if (self.sampleRate == 88200) {
        sampleRateCode = 2;
    } else if (self.sampleRate == 96000) {
        sampleRateCode = 3;
    } else {
        if (error) {
            *error = [NSError
                errorWithDomain:@"FrameHeaderErrorDomain"
                           code:104
                       userInfo:@{
                           NSLocalizedDescriptionKey : @"Invalid sample rate"
                       }];
        }
        return nil;
    }
    header |= (sampleRateCode << kSampleRateShift);

    uint32_t bitsCode = 0;
    if (self.bitsPerSample == 16) {
        bitsCode = 0;
    } else if (self.bitsPerSample == 24) {
        bitsCode = 1;
    } else if (self.bitsPerSample == 32) {
        bitsCode = 2;
    } else {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:105
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Invalid bits per sample"
                                     }];
        }
        return nil;
    }
    header |= (bitsCode << kBitsShift);

    header |= ((self.pts != nil ? 1 : 0) << kPTSShift);
    header |= ((self.frameID != nil ? 1 : 0) << kIDShift);
    header |= ((uint32_t)self.encoding << kEncodingShift);
    header |= ((self.endianness == EndiannessBig ? 1 : 0) << kEndianShift);
    header |= (((uint32_t)(self.channels - 1)) << kChannelsShift);
    header |= self.sampleSize;

    uint32_t beHeader = CFSwapInt32HostToBig(header);
    NSMutableData *data = [NSMutableData data];
    [data appendBytes:&beHeader length:4];

    if (self.frameID) {
        uint64_t idValue = [self.frameID unsignedLongLongValue];
        uint64_t beID = CFSwapInt64HostToBig(idValue);
        [data appendBytes:&beID length:8];
    }
    if (self.pts) {
        uint64_t ptsValue = [self.pts unsignedLongLongValue];
        uint64_t bePTS = CFSwapInt64HostToBig(ptsValue);
        [data appendBytes:&bePTS length:8];
    }

    return data;
}

+ (nullable instancetype)decodeFromData:(NSData *)data error:(NSError **)error {
    if (data.length < 4) {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:106
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Data too short for header"
                                     }];
        }
        return nil;
    }
    const uint8_t *bytes = data.bytes;
    uint32_t header;
    memcpy(&header, bytes, 4);
    header = CFSwapInt32BigToHost(header);

    if (((header & kMagicMask) >> kMagicShift) != kMagicWord) {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:107
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Invalid header magic word"
                                     }];
        }
        return nil;
    }

    uint32_t sampleRateCode = (header & kSampleRateMask) >> kSampleRateShift;
    uint32_t sampleRate = 0;
    switch (sampleRateCode) {
    case 0:
        sampleRate = 44100;
        break;
    case 1:
        sampleRate = 48000;
        break;
    case 2:
        sampleRate = 88200;
        break;
    case 3:
        sampleRate = 96000;
        break;
    default:
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:108
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Invalid sample rate code"
                                     }];
        }
        return nil;
    }

    uint32_t bitsCode = (header & kBitsMask) >> kBitsShift;
    uint8_t bitsPerSample = 0;
    switch (bitsCode) {
    case 0:
        bitsPerSample = 16;
        break;
    case 1:
        bitsPerSample = 24;
        break;
    case 2:
        bitsPerSample = 32;
        break;
    default:
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:109
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Invalid bits per sample code"
                                     }];
        }
        return nil;
    }

    BOOL hasPTS = ((header & kPTSMask) >> kPTSShift) == 1;
    BOOL hasID = ((header & kIDMask) >> kIDShift) == 1;

    uint32_t encodingCode = (header & kEncodingMask) >> kEncodingShift;
    EncodingFlag encoding = (EncodingFlag)encodingCode;

    uint32_t endianFlag = (header & kEndianMask) >> kEndianShift;
    Endianness endianness =
        (endianFlag == 0) ? EndiannessLittle : EndiannessBig;

    uint8_t channels =
        (uint8_t)(((header & kChannelsMask) >> kChannelsShift) + 1);
    uint16_t sampleSize = (uint16_t)(header & kSampleSizeMask);

    NSUInteger offset = 4;
    NSNumber *frameID = nil;
    if (hasID) {
        if (data.length < offset + 8) {
            if (error) {
                *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                             code:110
                                         userInfo:@{
                                             NSLocalizedDescriptionKey :
                                                 @"Data too short for frame ID"
                                         }];
            }
            return nil;
        }
        uint64_t idValue;
        memcpy(&idValue, bytes + offset, 8);
        idValue = CFSwapInt64BigToHost(idValue);
        frameID = @(idValue);
        offset += 8;
    }

    NSNumber *pts = nil;
    if (hasPTS) {
        if (data.length < offset + 8) {
            if (error) {
                *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                             code:111
                                         userInfo:@{
                                             NSLocalizedDescriptionKey :
                                                 @"Data too short for PTS"
                                         }];
            }
            return nil;
        }
        uint64_t ptsValue;
        memcpy(&ptsValue, bytes + offset, 8);
        ptsValue = CFSwapInt64BigToHost(ptsValue);
        pts = @(ptsValue);
        offset += 8;
    }

    return [[FrameHeader alloc] initWithEncoding:encoding
                                      sampleSize:sampleSize
                                      sampleRate:sampleRate
                                        channels:channels
                                   bitsPerSample:bitsPerSample
                                      endianness:endianness
                                         frameID:frameID
                                             pts:pts
                                           error:error];
}

- (NSUInteger)size {
    NSUInteger total = 4;
    if (self.frameID) {
        total += 8;
    }
    if (self.pts) {
        total += 8;
    }
    return total;
}

#pragma mark - Extraction Methods

+ (BOOL)validateHeaderData:(NSData *)data error:(NSError **)error {
    if (data.length < 4) {
        if (error) {
            *error = [NSError
                errorWithDomain:@"FrameHeaderErrorDomain"
                           code:112
                       userInfo:@{
                           NSLocalizedDescriptionKey : @"Header too small"
                       }];
        }
        return NO;
    }
    const uint8_t *bytes = data.bytes;
    uint32_t header;
    memcpy(&header, bytes, 4);
    header = CFSwapInt32BigToHost(header);

    if (((header & kMagicMask) >> kMagicShift) != kMagicWord) {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:107
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Invalid header magic word"
                                     }];
        }
        return NO;
    }

    uint32_t encodingCode = (header & kEncodingMask) >> kEncodingShift;
    if (encodingCode > 5) {
        if (error) {
            *error = [NSError
                errorWithDomain:@"FrameHeaderErrorDomain"
                           code:200
                       userInfo:@{
                           NSLocalizedDescriptionKey : @"Invalid encoding flag"
                       }];
        }
        return NO;
    }

    uint32_t sampleRateCode = (header & kSampleRateMask) >> kSampleRateShift;
    if (sampleRateCode > 3) {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:201
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Invalid sample rate code"
                                     }];
        }
        return NO;
    }

    uint8_t channels =
        (uint8_t)(((header & kChannelsMask) >> kChannelsShift) + 1);
    if (channels == 0 || channels > 16) {
        if (error) {
            *error = [NSError
                errorWithDomain:@"FrameHeaderErrorDomain"
                           code:202
                       userInfo:@{
                           NSLocalizedDescriptionKey : @"Invalid channel count"
                       }];
        }
        return NO;
    }

    uint32_t bitsCode = (header & kBitsMask) >> kBitsShift;
    if (bitsCode > 2) {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:203
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Invalid bits per sample code"
                                     }];
        }
        return NO;
    }

    return YES;
}

+ (uint16_t)extractSampleSizeFromData:(NSData *)data error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return 0;
    }
    const uint8_t *bytes = data.bytes;
    uint32_t header;
    memcpy(&header, bytes, 4);
    header = CFSwapInt32BigToHost(header);
    return (uint16_t)(header & kSampleSizeMask);
}

+ (EncodingFlag)extractEncodingFromData:(NSData *)data error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return 0;
    }
    const uint8_t *bytes = data.bytes;
    uint32_t header;
    memcpy(&header, bytes, 4);
    header = CFSwapInt32BigToHost(header);
    uint32_t encodingCode = (header & kEncodingMask) >> kEncodingShift;
    return (EncodingFlag)encodingCode;
}

+ (nullable NSNumber *)extractFrameIDFromData:(NSData *)data
                                        error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return nil;
    }
    const uint8_t *bytes = data.bytes;
    uint32_t header;
    memcpy(&header, bytes, 4);
    header = CFSwapInt32BigToHost(header);
    BOOL hasID = ((header & kIDMask) >> kIDShift) == 1;
    if (!hasID) {
        return nil;
    }
    if (data.length < 12) {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:113
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Header indicates ID present but "
                                             @"buffer too small"
                                     }];
        }
        return nil;
    }
    uint64_t idValue;
    memcpy(&idValue, bytes + 4, 8);
    idValue = CFSwapInt64BigToHost(idValue);
    return @(idValue);
}

+ (nullable NSNumber *)extractPTSFromData:(NSData *)data
                                    error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return nil;
    }
    const uint8_t *bytes = data.bytes;
    uint32_t header;
    memcpy(&header, bytes, 4);
    header = CFSwapInt32BigToHost(header);
    BOOL hasPTS = ((header & kPTSMask) >> kPTSShift) == 1;
    if (!hasPTS) {
        return nil;
    }
    BOOL hasID = ((header & kIDMask) >> kIDShift) == 1;
    NSUInteger ptsOffset = 4 + (hasID ? 8 : 0);
    if (data.length < ptsOffset + 8) {
        if (error) {
            *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                         code:114
                                     userInfo:@{
                                         NSLocalizedDescriptionKey :
                                             @"Header indicates PTS present "
                                             @"but buffer too small"
                                     }];
        }
        return nil;
    }
    uint64_t ptsValue;
    memcpy(&ptsValue, bytes + ptsOffset, 8);
    ptsValue = CFSwapInt64BigToHost(ptsValue);
    return @(ptsValue);
}

#pragma mark - Patch Methods

+ (BOOL)patchBitsPerSampleInData:(NSMutableData *)data
                            bits:(uint8_t)bits
                           error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return NO;
    }
    uint32_t bitsCode = 0;
    if (bits == 16) {
        bitsCode = 0;
    } else if (bits == 24) {
        bitsCode = 1;
    } else if (bits == 32) {
        bitsCode = 2;
    } else {
        if (error) {
            *error =
                [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                    code:115
                                userInfo:@{
                                    NSLocalizedDescriptionKey :
                                        @"Bits per sample must be 16, 24, or 32"
                                }];
        }
        return NO;
    }
    uint8_t *mutableBytes = (uint8_t *)data.mutableBytes;
    uint32_t header;
    memcpy(&header, mutableBytes, 4);
    header = CFSwapInt32BigToHost(header);
    header &= ~kBitsMask;
    header |= ((bitsCode << kBitsShift) & kBitsMask);
    uint32_t beHeader = CFSwapInt32HostToBig(header);
    memcpy(mutableBytes, &beHeader, 4);
    return YES;
}

+ (BOOL)patchSampleSizeInData:(NSMutableData *)data
                newSampleSize:(uint16_t)newSampleSize
                        error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return NO;
    }
    if (newSampleSize > kMaxSampleSize) {
        if (error) {
            NSString *msg = [NSString
                stringWithFormat:@"Sample size exceeds maximum value (%u)",
                                 kMaxSampleSize];
            *error =
                [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                    code:116
                                userInfo:@{NSLocalizedDescriptionKey : msg}];
        }
        return NO;
    }
    uint8_t *mutableBytes = (uint8_t *)data.mutableBytes;
    uint32_t header;
    memcpy(&header, mutableBytes, 4);
    header = CFSwapInt32BigToHost(header);
    header &= ~kSampleSizeMask;
    header |= newSampleSize;
    uint32_t beHeader = CFSwapInt32HostToBig(header);
    memcpy(mutableBytes, &beHeader, 4);
    return YES;
}

+ (BOOL)patchEncodingInData:(NSMutableData *)data
                   encoding:(EncodingFlag)encoding
                      error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return NO;
    }
    uint8_t *mutableBytes = (uint8_t *)data.mutableBytes;
    uint32_t header;
    memcpy(&header, mutableBytes, 4);
    header = CFSwapInt32BigToHost(header);
    header &= ~kEncodingMask;
    header |= (((uint32_t)encoding << kEncodingShift) & kEncodingMask);
    uint32_t beHeader = CFSwapInt32HostToBig(header);
    memcpy(mutableBytes, &beHeader, 4);
    return YES;
}

+ (BOOL)patchSampleRateInData:(NSMutableData *)data
                   sampleRate:(uint32_t)sampleRate
                        error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return NO;
    }
    uint32_t rateCode = 0;
    if (sampleRate == 44100) {
        rateCode = 0;
    } else if (sampleRate == 48000) {
        rateCode = 1;
    } else if (sampleRate == 88200) {
        rateCode = 2;
    } else if (sampleRate == 96000) {
        rateCode = 3;
    } else {
        if (error) {
            NSString *msg = [NSString
                stringWithFormat:@"Invalid sample rate: %u. Must be one of: "
                                 @"[44100, 48000, 88200, 96000]",
                                 sampleRate];
            *error =
                [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                    code:117
                                userInfo:@{NSLocalizedDescriptionKey : msg}];
        }
        return NO;
    }
    uint8_t *mutableBytes = (uint8_t *)data.mutableBytes;
    uint32_t header;
    memcpy(&header, mutableBytes, 4);
    header = CFSwapInt32BigToHost(header);
    header &= ~kSampleRateMask;
    header |= ((rateCode << kSampleRateShift) & kSampleRateMask);
    uint32_t beHeader = CFSwapInt32HostToBig(header);
    memcpy(mutableBytes, &beHeader, 4);
    return YES;
}

+ (BOOL)patchChannelsInData:(NSMutableData *)data
                   channels:(uint8_t)channels
                      error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return NO;
    }
    if (channels == 0 || channels > 16) {
        if (error) {
            *error = [NSError
                errorWithDomain:@"FrameHeaderErrorDomain"
                           code:118
                       userInfo:@{
                           NSLocalizedDescriptionKey :
                               @"Channel count must be between 1 and 16"
                       }];
        }
        return NO;
    }
    uint8_t *mutableBytes = (uint8_t *)data.mutableBytes;
    uint32_t header;
    memcpy(&header, mutableBytes, 4);
    header = CFSwapInt32BigToHost(header);
    header &= ~kChannelsMask;
    header |= (((uint32_t)(channels - 1) << kChannelsShift) & kChannelsMask);
    uint32_t beHeader = CFSwapInt32HostToBig(header);
    memcpy(mutableBytes, &beHeader, 4);
    return YES;
}

+ (BOOL)patchFrameIDInData:(NSMutableData *)data
                   frameID:(nullable NSNumber *)frameID
                     error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return NO;
    }
    uint8_t *mutableBytes = (uint8_t *)data.mutableBytes;
    uint32_t header;
    memcpy(&header, mutableBytes, 4);
    header = CFSwapInt32BigToHost(header);
    header &= ~kIDMask;
    if (frameID) {
        header |= (1 << kIDShift);
        if (data.length < 12) {
            if (error) {
                *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                             code:119
                                         userInfo:@{
                                             NSLocalizedDescriptionKey :
                                                 @"Buffer too small to add ID"
                                         }];
            }
            return NO;
        }
        uint64_t idValue = [frameID unsignedLongLongValue];
        uint64_t beID = CFSwapInt64HostToBig(idValue);
        memcpy(mutableBytes + 4, &beID, 8);
    }
    uint32_t beHeader = CFSwapInt32HostToBig(header);
    memcpy(mutableBytes, &beHeader, 4);
    return YES;
}

+ (BOOL)patchPTSInData:(NSMutableData *)data
                   pts:(nullable NSNumber *)pts
                 error:(NSError **)error {
    if (![self validateHeaderData:data error:error]) {
        return NO;
    }
    uint8_t *mutableBytes = (uint8_t *)data.mutableBytes;
    uint32_t header;
    memcpy(&header, mutableBytes, 4);
    header = CFSwapInt32BigToHost(header);
    header &= ~kPTSMask;
    BOOL hasID = ((header & kIDMask) >> kIDShift) == 1;
    NSUInteger ptsOffset = 4 + (hasID ? 8 : 0);
    if (pts) {
        header |= (1 << kPTSShift);
        if (data.length < ptsOffset + 8) {
            if (error) {
                *error = [NSError errorWithDomain:@"FrameHeaderErrorDomain"
                                             code:120
                                         userInfo:@{
                                             NSLocalizedDescriptionKey :
                                                 @"Buffer too small to add PTS"
                                         }];
            }
            return NO;
        }
        uint64_t ptsValue = [pts unsignedLongLongValue];
        uint64_t bePTS = CFSwapInt64HostToBig(ptsValue);
        memcpy(mutableBytes + ptsOffset, &bePTS, 8);
    }
    uint32_t beHeader = CFSwapInt32HostToBig(header);
    memcpy(mutableBytes, &beHeader, 4);
    return YES;
}

@end

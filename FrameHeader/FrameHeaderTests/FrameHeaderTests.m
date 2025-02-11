#import "../FrameHeader/FrameHeader.h" // Adjust the path if needed
#import <XCTest/XCTest.h>

// Define macros for our magic word constants (must match implementation)
#define FRAMEHEADER_MAGIC_WORD 0x2A
#define FRAMEHEADER_MAGIC_SHIFT 26
#define FRAMEHEADER_MAGIC_MASK (0x3F << FRAMEHEADER_MAGIC_SHIFT)

@interface FrameHeaderTests : XCTestCase
@end

@implementation FrameHeaderTests

#pragma mark - Helper Methods

// Creates a header with no optional fields (neither frameID nor pts)
- (NSData *)createTestHeader {
    NSError *error = nil;
    FrameHeader *header =
        [[FrameHeader alloc] initWithEncoding:EncodingFlagPCMSigned
                                   sampleSize:1024
                                   sampleRate:48000
                                     channels:2
                                bitsPerSample:24
                                   endianness:EndiannessLittle
                                      frameID:nil
                                          pts:nil
                                        error:&error];
    XCTAssertNotNil(header, @"Header creation should succeed: %@", error);
    NSData *encoded = [header encodeWithError:&error];
    XCTAssertNotNil(encoded, @"Encoding should succeed: %@", error);
    return encoded;
}

// Creates a header with pts only.
- (NSData *)createHeaderWithPTS {
    NSError *error = nil;
    FrameHeader *header =
        [[FrameHeader alloc] initWithEncoding:EncodingFlagPCMSigned
                                   sampleSize:1024
                                   sampleRate:48000
                                     channels:2
                                bitsPerSample:24
                                   endianness:EndiannessLittle
                                      frameID:nil
                                          pts:@(0x1234567890ABCDEF)
                                        error:&error];
    XCTAssertNotNil(header, @"Header with pts should be created: %@", error);
    NSData *encoded = [header encodeWithError:&error];
    XCTAssertNotNil(encoded, @"Encoding should succeed: %@", error);
    return encoded;
}

// Creates a header with both frameID and pts.
- (NSData *)createHeaderWithIDAndPTS {
    NSError *error = nil;
    FrameHeader *header =
        [[FrameHeader alloc] initWithEncoding:EncodingFlagPCMSigned
                                   sampleSize:1024
                                   sampleRate:48000
                                     channels:2
                                bitsPerSample:24
                                   endianness:EndiannessLittle
                                      frameID:@(0xDEADBEEF)
                                          pts:@(0xFEEDFACE)
                                        error:&error];
    XCTAssertNotNil(header, @"Header with id and pts should be created: %@",
                    error);
    NSData *encoded = [header encodeWithError:&error];
    XCTAssertNotNil(encoded, @"Encoding should succeed: %@", error);
    return encoded;
}

#pragma mark - Extraction Failure Tests

- (void)testExtractionFailuresForInvalidHeader {
    NSData *headerData = [self createHeaderWithPTS];

    // Corrupt the magic word.
    NSMutableData *invalidHeader = [headerData mutableCopy];
    uint8_t *bytes = invalidHeader.mutableBytes;
    bytes[0] = 0; // Corrupt the magic word.

    NSError *error = nil;
    NSNumber *extractedPTS = [FrameHeader extractPTSFromData:invalidHeader
                                                       error:&error];
    XCTAssertNil(extractedPTS,
                 @"Extraction should fail with an invalid header");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");

    // Test truncated header.
    NSData *truncated = [headerData subdataWithRange:NSMakeRange(0, 4)];
    error = nil;
    extractedPTS = [FrameHeader extractPTSFromData:truncated error:&error];
    XCTAssertNil(
        extractedPTS,
        @"Extraction should fail for truncated header with pts flag set");
    XCTAssertNotNil(error, @"Error should be provided for truncated header");

    error = nil;
    uint16_t sampleSize = [FrameHeader extractSampleSizeFromData:invalidHeader
                                                           error:&error];
    XCTAssertEqual(sampleSize, 0,
                   @"Extraction should fail with invalid header (sample size "
                   @"returned as 0)");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");

    error = nil;
    EncodingFlag encoding = [FrameHeader extractEncodingFromData:invalidHeader
                                                           error:&error];
    XCTAssertEqual(
        encoding, 0,
        @"Extraction should fail with invalid header (encoding returned as 0)");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");

    error = nil;
    NSNumber *extractedID = [FrameHeader extractFrameIDFromData:invalidHeader
                                                          error:&error];
    XCTAssertNil(
        extractedID,
        @"Extraction should fail with invalid header (frameID should be nil)");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");
}

#pragma mark - Magic Word Tests

- (void)testMagicWordFailures {
    NSError *error = nil;

    // Helper: create a fresh valid header buffer.
    NSData *validHeaderData = [self createTestHeader];

    // Off by one higher.
    NSMutableData *buffer = [validHeaderData mutableCopy];
    {
        uint32_t header;
        memcpy(&header, buffer.mutableBytes, 4);
        header = CFSwapInt32BigToHost(header);
        header &= ~FRAMEHEADER_MAGIC_MASK;
        header |= ((FRAMEHEADER_MAGIC_WORD + 1) << FRAMEHEADER_MAGIC_SHIFT);
        uint32_t beHeader = CFSwapInt32HostToBig(header);
        memcpy(buffer.mutableBytes, &beHeader, 4);
    }
    error = nil;
    FrameHeader *result = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertNil(result, @"Decoding should fail if magic word is too high");
    XCTAssertNotNil(error,
                    @"Error should be provided when magic word is too high");

    // Off by one lower.
    buffer = [[self createTestHeader] mutableCopy];
    {
        uint32_t header;
        memcpy(&header, buffer.mutableBytes, 4);
        header = CFSwapInt32BigToHost(header);
        header &= ~FRAMEHEADER_MAGIC_MASK;
        header |= ((FRAMEHEADER_MAGIC_WORD - 1) << FRAMEHEADER_MAGIC_SHIFT);
        uint32_t beHeader = CFSwapInt32HostToBig(header);
        memcpy(buffer.mutableBytes, &beHeader, 4);
    }
    error = nil;
    result = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertNil(result, @"Decoding should fail if magic word is too low");
    XCTAssertNotNil(error,
                    @"Error should be provided when magic word is too low");

    // Shifted right by one bit.
    buffer = [[self createTestHeader] mutableCopy];
    {
        uint32_t header;
        memcpy(&header, buffer.mutableBytes, 4);
        header = CFSwapInt32BigToHost(header);
        header &= ~FRAMEHEADER_MAGIC_MASK;
        header |= ((FRAMEHEADER_MAGIC_WORD >> 1) << FRAMEHEADER_MAGIC_SHIFT);
        uint32_t beHeader = CFSwapInt32HostToBig(header);
        memcpy(buffer.mutableBytes, &beHeader, 4);
    }
    error = nil;
    result = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertNil(result,
                 @"Decoding should fail if magic word is shifted right");
    XCTAssertNotNil(
        error, @"Error should be provided when magic word is shifted right");

    // Shifted left by one bit.
    buffer = [[self createTestHeader] mutableCopy];
    {
        uint32_t header;
        memcpy(&header, buffer.mutableBytes, 4);
        header = CFSwapInt32BigToHost(header);
        header &= ~FRAMEHEADER_MAGIC_MASK;
        header |= ((FRAMEHEADER_MAGIC_WORD << 1) << FRAMEHEADER_MAGIC_SHIFT);
        uint32_t beHeader = CFSwapInt32HostToBig(header);
        memcpy(buffer.mutableBytes, &beHeader, 4);
    }
    error = nil;
    result = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertNil(result, @"Decoding should fail if magic word is shifted left");
    XCTAssertNotNil(
        error, @"Error should be provided when magic word is shifted left");

    // Magic word at wrong bit position (shifted by one extra bit).
    buffer = [[self createTestHeader] mutableCopy];
    {
        uint32_t header;
        memcpy(&header, buffer.mutableBytes, 4);
        header = CFSwapInt32BigToHost(header);
        header &= ~FRAMEHEADER_MAGIC_MASK;
        header |= (FRAMEHEADER_MAGIC_WORD << (FRAMEHEADER_MAGIC_SHIFT + 1));
        uint32_t beHeader = CFSwapInt32HostToBig(header);
        memcpy(buffer.mutableBytes, &beHeader, 4);
    }
    error = nil;
    result = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertNil(
        result, @"Decoding should fail if magic word is at the wrong position");
    XCTAssertNotNil(
        error,
        @"Error should be provided when magic word is at the wrong position");
}

#pragma mark - Other Tests

- (void)testEncodingRoundtripWithPTS {
    NSError *error = nil;
    FrameHeader *original =
        [[FrameHeader alloc] initWithEncoding:EncodingFlagOpus
                                   sampleSize:2048
                                   sampleRate:48000
                                     channels:8
                                bitsPerSample:16
                                   endianness:EndiannessLittle
                                      frameID:@(0xDEADBEEF)
                                          pts:@(0xCAFEBABE)
                                        error:&error];
    XCTAssertNotNil(original, @"Header creation should succeed: %@", error);
    NSData *encoded = [original encodeWithError:&error];
    XCTAssertNotNil(encoded, @"Encoding should succeed: %@", error);
    FrameHeader *decoded = [FrameHeader decodeFromData:encoded error:&error];
    XCTAssertNotNil(decoded, @"Decoding should succeed: %@", error);
    XCTAssertEqualObjects(decoded.pts, original.pts,
                          @"PTS should roundtrip correctly");
    XCTAssertEqualObjects(decoded.frameID, original.frameID,
                          @"frameID should roundtrip correctly");
    XCTAssertEqual(decoded.size, original.size, @"Sizes should match");
    XCTAssertEqual(encoded.length, decoded.size,
                   @"Encoded length should match decoded size");
}

- (void)testPatchOperations {
    NSError *error = nil;
    NSData *baseHeaderData = [self createTestHeader];

    // Patch sample size.
    NSMutableData *buffer = [baseHeaderData mutableCopy];
    BOOL success = [FrameHeader patchSampleSizeInData:buffer
                                        newSampleSize:2048
                                                error:&error];
    XCTAssertTrue(success, @"Patching sample size should succeed");
    FrameHeader *updated = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertEqual(updated.sampleSize, 2048,
                   @"Sample size should be patched to 2048");

    // Patch encoding.
    buffer = [baseHeaderData mutableCopy];
    success = [FrameHeader patchEncodingInData:buffer
                                      encoding:EncodingFlagFLAC
                                         error:&error];
    XCTAssertTrue(success, @"Patching encoding should succeed");
    updated = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertEqual(updated.encoding, EncodingFlagFLAC,
                   @"Encoding should be patched to FLAC");

    // Patch sample rate.
    buffer = [baseHeaderData mutableCopy];
    success = [FrameHeader patchSampleRateInData:buffer
                                      sampleRate:96000
                                           error:&error];
    XCTAssertTrue(success, @"Patching sample rate should succeed");
    updated = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertEqual(updated.sampleRate, 96000,
                   @"Sample rate should be patched to 96000");

    // Patch bits per sample.
    buffer = [baseHeaderData mutableCopy];
    success = [FrameHeader patchBitsPerSampleInData:buffer
                                               bits:32
                                              error:&error];
    XCTAssertTrue(success, @"Patching bits per sample should succeed");
    updated = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertEqual(updated.bitsPerSample, 32,
                   @"Bits per sample should be patched to 32");

    // Patch channels.
    buffer = [baseHeaderData mutableCopy];
    success = [FrameHeader patchChannelsInData:buffer channels:16 error:&error];
    XCTAssertTrue(success, @"Patching channels should succeed");
    updated = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertEqual(updated.channels, 16, @"Channels should be patched to 16");

    // For optional fields, use a header that already includes them.
    NSData *headerWithOptional = [self createHeaderWithIDAndPTS];

    // Patch pts.
    buffer = [headerWithOptional mutableCopy];
    success = [FrameHeader patchPTSInData:buffer
                                      pts:@(0xCAFEBABE)
                                    error:&error];
    XCTAssertTrue(success, @"Patching pts should succeed");
    updated = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertEqualObjects(updated.pts, @(0xCAFEBABE),
                          @"PTS should be patched to new value");

    // Patch frameID.
    buffer = [headerWithOptional mutableCopy];
    success = [FrameHeader patchFrameIDInData:buffer
                                      frameID:@(0xFEEDFACE)
                                        error:&error];
    XCTAssertTrue(success, @"Patching frameID should succeed");
    updated = [FrameHeader decodeFromData:buffer error:&error];
    XCTAssertEqualObjects(updated.frameID, @(0xFEEDFACE),
                          @"frameID should be patched to new value");
}

- (void)testExtractOperations {
    NSData *headerData = [self createHeaderWithIDAndPTS];
    NSError *error = nil;

    uint16_t extractedSampleSize =
        [FrameHeader extractSampleSizeFromData:headerData error:&error];
    XCTAssertEqual(extractedSampleSize, 1024,
                   @"Extracted sample size should be 1024");

    EncodingFlag extractedEncoding =
        [FrameHeader extractEncodingFromData:headerData error:&error];
    XCTAssertEqual(extractedEncoding, EncodingFlagPCMSigned,
                   @"Extracted encoding should be PCMSigned");

    NSNumber *extractedID = [FrameHeader extractFrameIDFromData:headerData
                                                          error:&error];
    XCTAssertEqualObjects(extractedID, @(0xDEADBEEF),
                          @"Extracted frameID should match");

    NSNumber *extractedPTS = [FrameHeader extractPTSFromData:headerData
                                                       error:&error];
    XCTAssertEqualObjects(extractedPTS, @(0xFEEDFACE),
                          @"Extracted pts should match");

    // Test invalid header: corrupt magic word.
    NSMutableData *invalidHeader = [headerData mutableCopy];
    uint8_t *bytes = invalidHeader.mutableBytes;
    bytes[0] = 0;
    error = nil;
    uint16_t resultSampleSize =
        [FrameHeader extractSampleSizeFromData:invalidHeader error:&error];
    XCTAssertEqual(resultSampleSize, 0,
                   @"Extraction should fail with invalid header (sample size "
                   @"returned as 0)");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");

    error = nil;
    extractedEncoding = [FrameHeader extractEncodingFromData:invalidHeader
                                                       error:&error];
    XCTAssertEqual(
        extractedEncoding, 0,
        @"Extraction should fail with invalid header (encoding returned as 0)");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");

    error = nil;
    extractedID = [FrameHeader extractFrameIDFromData:invalidHeader
                                                error:&error];
    XCTAssertNil(
        extractedID,
        @"Extraction should fail with invalid header (frameID should be nil)");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");

    error = nil;
    extractedPTS = [FrameHeader extractPTSFromData:invalidHeader error:&error];
    XCTAssertNil(
        extractedPTS,
        @"Extraction should fail with invalid header (pts should be nil)");
    XCTAssertNotNil(error, @"Error should be provided when extraction fails");
}

- (void)testPatchValidation {
    NSMutableData *headerData = [[self createTestHeader] mutableCopy];
    NSError *error = nil;

    BOOL success = [FrameHeader patchSampleSizeInData:headerData
                                        newSampleSize:5000
                                                error:&error];
    XCTAssertFalse(success, @"Should fail to patch sample size above maximum");
    XCTAssertNotNil(
        error, @"Error should be provided when patching invalid sample size");

    success = [FrameHeader patchSampleRateInData:headerData
                                      sampleRate:192000
                                           error:&error];
    XCTAssertFalse(success, @"Should fail to patch invalid sample rate");
    XCTAssertNotNil(
        error, @"Error should be provided when patching invalid sample rate");

    success = [FrameHeader patchChannelsInData:headerData
                                      channels:17
                                         error:&error];
    XCTAssertFalse(success, @"Should fail to patch invalid channels (>16)");
    XCTAssertNotNil(error,
                    @"Error should be provided when patching invalid channels");

    success = [FrameHeader patchChannelsInData:headerData
                                      channels:0
                                         error:&error];
    XCTAssertFalse(success, @"Should fail to patch invalid channels (0)");
    XCTAssertNotNil(error,
                    @"Error should be provided when patching invalid channels");

    success = [FrameHeader patchBitsPerSampleInData:headerData
                                               bits:20
                                              error:&error];
    XCTAssertFalse(success, @"Should fail to patch invalid bits per sample");
    XCTAssertNotNil(
        error,
        @"Error should be provided when patching invalid bits per sample");
}

- (void)testSampleSizeExtraction {
    NSError *error = nil;
    FrameHeader *header =
        [[FrameHeader alloc] initWithEncoding:EncodingFlagPCMSigned
                                   sampleSize:1024
                                   sampleRate:48000
                                     channels:2
                                bitsPerSample:24
                                   endianness:EndiannessLittle
                                      frameID:nil
                                          pts:nil
                                        error:&error];
    XCTAssertNotNil(header, @"Header creation should succeed: %@", error);
    NSData *encoded = [header encodeWithError:&error];
    XCTAssertNotNil(encoded, @"Encoding should succeed: %@", error);
    uint16_t extracted = [FrameHeader extractSampleSizeFromData:encoded
                                                          error:&error];
    XCTAssertEqual(extracted, 1024, @"Extracted sample size should be 1024");
    FrameHeader *decoded = [FrameHeader decodeFromData:encoded error:&error];
    XCTAssertEqual(decoded.sampleSize, 1024,
                   @"Decoded sample size should be 1024");
}

- (void)testBitLayout {
    NSError *error = nil;
    // Create a header with maximum field values.
    FrameHeader *header = [[FrameHeader alloc]
        initWithEncoding:EncodingFlagPCMSigned
              sampleSize:0xFFF
              sampleRate:48000
                channels:16
           bitsPerSample:32
              endianness:EndiannessBig
                 frameID:@(1)
                     pts:@(1)
                   error:&error];
    XCTAssertNotNil(
        header, @"Header creation with max values should succeed: %@", error);
    NSData *encoded = [header encodeWithError:&error];
    XCTAssertNotNil(encoded, @"Encoding should succeed: %@", error);
    FrameHeader *decoded = [FrameHeader decodeFromData:encoded error:&error];
    XCTAssertEqual(decoded.sampleSize, 0xFFF,
                   @"Max sample size should be preserved");
    XCTAssertEqual(decoded.channels, 16, @"Max channels should be preserved");
    XCTAssertEqual(decoded.bitsPerSample, 32,
                   @"Max bits per sample should be preserved");
    XCTAssertEqual(decoded.endianness, EndiannessBig,
                   @"Endianness should be Big");
    XCTAssertNotNil(decoded.frameID, @"frameID should be present");
    XCTAssertNotNil(decoded.pts, @"PTS should be present");
}

- (void)testValidOpusAndFlacSampleSizesWithVariedPTSAndIDs {
    NSArray<NSNumber *> *opusSampleSizes = @[@80, @160, @240, @480, @960, @1920, @2880];
    NSArray<NSNumber *> *flacSampleSizes = @[@512, @1024, @2048];
    NSArray<NSNumber *> *sampleRates = @[@44100, @48000, @88200, @96000];
    NSArray<NSNumber *> *channelsList = @[@1, @2, @8, @16];
    NSArray<NSNumber *> *bitsList = @[@16, @24, @32];
    NSArray<NSNumber *> *endiannessList = @[@(EndiannessLittle), @(EndiannessBig)];
    NSArray<NSNumber *> *ptsValues = @[@1670000000000000ULL, @1671000000000000ULL,
                                       @1672000000000000ULL, @1673000000000000ULL,
                                       @1674000000000000ULL, @1675000000000000ULL];
    NSArray<NSNumber *> *idValues = @[@(UINT64_MAX), @0x0123456789ABCDEFULL,
                                      @0xDEADBEEFDEADBEEFULL, @0, @1, @42];
    
    for (NSNumber *encodingNum in @[@(EncodingFlagOpus), @(EncodingFlagFLAC)]) {
        EncodingFlag encoding = encodingNum.unsignedIntValue;
        NSArray<NSNumber *> *sampleSizes = (encoding == EncodingFlagOpus) ? opusSampleSizes : flacSampleSizes;
        for (NSNumber *sampleSizeNum in sampleSizes) {
            uint16_t sampleSize = sampleSizeNum.unsignedShortValue;
            for (NSNumber *sampleRateNum in sampleRates) {
                uint32_t sampleRate = sampleRateNum.unsignedIntValue;
                for (NSNumber *channelsNum in channelsList) {
                    uint8_t channels = channelsNum.unsignedCharValue;
                    for (NSNumber *bitsNum in bitsList) {
                        uint8_t bits = bitsNum.unsignedCharValue;
                        for (NSNumber *endianNum in endiannessList) {
                            Endianness endian = endianNum.unsignedIntValue;
                            for (NSNumber *idVal in idValues) {
                                for (NSNumber *ptsVal in ptsValues) {
                                    NSError *error = nil;
                                    FrameHeader *header = [[FrameHeader alloc] initWithEncoding:encoding
                                                                                    sampleSize:sampleSize
                                                                                    sampleRate:sampleRate
                                                                                      channels:channels
                                                                               bitsPerSample:bits
                                                                                  endianness:endian
                                                                                     frameID:idVal
                                                                                         pts:ptsVal
                                                                                       error:&error];
                                    XCTAssertNotNil(header,
                                                  @"Failed to create header for encoding: %u, sampleSize: %u, sampleRate: %u, channels: %u, bits: %u, endianness: %u, frameID: %@, pts: %@",
                                                  encoding, sampleSize, sampleRate, channels, bits, endian, idVal, ptsVal);
                                    
                                    NSData *encoded = [header encodeWithError:&error];
                                    XCTAssertNotNil(encoded,
                                                  @"Failed to encode header for encoding: %u, sampleSize: %u, sampleRate: %u, channels: %u, bits: %u, endianness: %u, frameID: %@, pts: %@",
                                                  encoding, sampleSize, sampleRate, channels, bits, endian, idVal, ptsVal);
                                    
                                    FrameHeader *decoded = [FrameHeader decodeFromData:encoded error:&error];
                                    XCTAssertNotNil(decoded,
                                                  @"Failed to decode header for encoding: %u, sampleSize: %u, sampleRate: %u, channels: %u, bits: %u, endianness: %u, frameID: %@, pts: %@",
                                                  encoding, sampleSize, sampleRate, channels, bits, endian, idVal, ptsVal);
                                    
                                    XCTAssertEqual(decoded.encoding, encoding, @"Encoding mismatch");
                                    XCTAssertEqual(decoded.sampleSize, sampleSize, @"Sample size mismatch");
                                    XCTAssertEqual(decoded.sampleRate, sampleRate, @"Sample rate mismatch");
                                    XCTAssertEqual(decoded.channels, channels, @"Channels mismatch");
                                    XCTAssertEqual(decoded.bitsPerSample, bits, @"Bits per sample mismatch");
                                    XCTAssertEqual(decoded.endianness, endian, @"Endianness mismatch");
                                    XCTAssertEqualObjects(decoded.frameID, idVal, @"frameID mismatch");
                                    XCTAssertEqualObjects(decoded.pts, ptsVal, @"PTS mismatch");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@end

import PackageDescription

let package = Package(
    name: "FrameHeader",
    platforms: [
        .iOS(.v11)
    ],
    products: [
        .library(
            name: "FrameHeader",
            targets: ["FrameHeader"])
    ],
    targets: [
        .target(
            name: "FrameHeader",
            path: "FrameHeader/FrameHeader",
            publicHeadersPath: "."
        ),
        .testTarget(
            name: "FrameHeaderTests",
            dependencies: ["FrameHeader"],
            path: "FrameHeader/FrameHeaderTests"
        )
    ]
)


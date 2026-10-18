// swift-tools-version: 6.2

import PackageDescription

let settings: [SwiftSetting] = [
  .swiftLanguageMode(.v6),
  .treatAllWarnings(as: .error),
  .strictMemorySafety(),
]

let package = Package(
  name: "avirtvmd",
  platforms: [.macOS("15.0")],
  products: [
    .executable(name: "avirtvmd", targets: ["AvirtVMDCommand"])
  ],
  dependencies: [
    .package(url: "https://github.com/apple/swift-argument-parser.git", from: "1.8.2"),
    .package(url: "https://github.com/apple/swift-protobuf.git", from: "1.36.0"),
    .package(url: "https://github.com/apple/swift-nio.git", from: "2.104.0"),
  ],
  targets: [
    .executableTarget(
      name: "AvirtVMDCommand",
      dependencies: [
        "AvirtVMD",
        .product(name: "ArgumentParser", package: "swift-argument-parser"),
      ],
      swiftSettings: settings
    ),
    .target(
      name: "AvirtVMD",
      dependencies: [
        .product(name: "SwiftProtobuf", package: "swift-protobuf"),
        .product(name: "NIOCore", package: "swift-nio"),
        .product(name: "NIOPosix", package: "swift-nio"),
      ],
      resources: [.copy("swift-protobuf-config.json")],
      swiftSettings: settings,
      plugins: [
        .plugin(name: "SwiftProtobufPlugin", package: "swift-protobuf")
      ]
    ),
    .testTarget(
      name: "AvirtVMDTests",
      dependencies: [
        "AvirtVMD",
        .product(name: "NIOEmbedded", package: "swift-nio"),
        .product(name: "SwiftProtobuf", package: "swift-protobuf"),
      ],
      resources: [.copy("fixtures")],
      swiftSettings: settings
    ),
  ]
)

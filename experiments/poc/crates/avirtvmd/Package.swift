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
    .package(url: "https://github.com/apple/containerization.git", exact: "0.53.0"),
    .package(url: "https://github.com/apple/swift-argument-parser.git", from: "1.8.2"),
    .package(url: "https://github.com/apple/swift-protobuf.git", from: "1.36.0"),
    .package(url: "https://github.com/apple/swift-nio.git", from: "2.104.0"),
    .package(url: "https://github.com/apple/swift-system.git", from: "1.5.0"),
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
        .product(name: "Containerization", package: "containerization"),
        .product(name: "ContainerizationArchive", package: "containerization"),
        .product(name: "ContainerizationEXT4", package: "containerization"),
        .product(name: "ContainerizationOCI", package: "containerization"),
        .product(name: "SwiftProtobuf", package: "swift-protobuf"),
        .product(name: "NIOCore", package: "swift-nio"),
        .product(name: "NIOPosix", package: "swift-nio"),
        .product(name: "SystemPackage", package: "swift-system"),
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
        .product(name: "ContainerizationArchive", package: "containerization"),
        .product(name: "ContainerizationEXT4", package: "containerization"),
        .product(name: "NIOEmbedded", package: "swift-nio"),
        .product(name: "SystemPackage", package: "swift-system"),
        .product(name: "SwiftProtobuf", package: "swift-protobuf"),
      ],
      resources: [.copy("fixtures")],
      swiftSettings: settings
    ),
  ]
)

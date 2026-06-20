// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TokenStepSwift",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "TokenStepSwift", targets: ["TokenStepSwift"])
    ],
    targets: [
        .executableTarget(name: "TokenStepSwift")
    ]
)

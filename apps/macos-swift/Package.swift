// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "OstrichConnectMac",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "OstrichConnectApp", targets: ["OstrichConnectApp"])
    ],
    targets: [
        .systemLibrary(
            name: "COstrichFFI",
            path: "Sources/COstrichFFI"
        ),
        .executableTarget(
            name: "OstrichConnectApp",
            dependencies: ["COstrichFFI"]
        )
    ]
)

// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "CasitaAssistantMenu",
    platforms: [
        .macOS(.v12)
    ],
    targets: [
        .executableTarget(
            name: "CasitaAssistantMenu",
            path: "Sources/CasitaAssistantMenu"
        )
    ]
)

// swift-tools-version:5.3
import PackageDescription

let package = Package(
    name: "tauri-plugin-folder-picker",
    platforms: [
        .iOS(.v14)
    ],
    products: [
        .library(
            name: "tauri-plugin-folder-picker",
            type: .static,
            targets: ["tauri-plugin-folder-picker"])
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-folder-picker",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources")
    ]
)

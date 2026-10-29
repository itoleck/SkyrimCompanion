//! Windows version resource for map-extract.exe (version from `version.txt`,
//! bumped by create-release.ps1).

#[path = "../../build-support/version_resource.rs"]
mod version_resource;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    version_resource::embed(&version_resource::VersionInfo {
        file_description: "Skyrim Companion map builder",
        original_filename: "map-extract.exe",
        internal_name: "map-extract",
        dll: false,
    });
}

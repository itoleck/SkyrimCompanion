//! Windows version resource for skyrim-companion.exe (version from `version.txt`,
//! bumped by create-release.ps1).

#[path = "../../build-support/version_resource.rs"]
mod version_resource;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    version_resource::embed(&version_resource::VersionInfo {
        file_description: "Skyrim Companion second-monitor app",
        original_filename: "skyrim-companion.exe",
        internal_name: "skyrim-companion",
        dll: false,
    });
}

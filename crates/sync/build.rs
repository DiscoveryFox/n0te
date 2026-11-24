//! Windows: icon, version info and manifest of n0te-sync.exe (packaging/windows/resources.rs).
//! Nothing happens for other targets.

#[path = "../../packaging/windows/resources.rs"]
mod resources;

fn main() {
    resources::embed(&[resources::Program { bin: "n0te-sync", description: "n0te sync", file: "n0te-sync.exe", icon: "packaging/icons/n0te-sync.ico" }]);
}

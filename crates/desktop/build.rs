//! Windows: each of the two programs gets its own icon, version info and manifest
//! (packaging/windows/resources.rs). Nothing happens for other targets.

#[path = "../../packaging/windows/resources.rs"]
mod resources;

use resources::Program;

fn main() {
    resources::embed(&[
        Program { bin: "n0te", description: "n0te Laptop", file: "n0te-laptop.exe", icon: "packaging/icons/n0te-laptop.ico" },
        Program { bin: "n0te-tablet", description: "n0te Tablet", file: "n0te-tablet.exe", icon: "packaging/icons/n0te-tablet.ico" },
    ]);
}

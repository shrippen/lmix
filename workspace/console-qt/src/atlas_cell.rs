//! The cells of the tile picture, drawn from its texture (see
//! `cpp/atlas_cell.h`).

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("console-qt/cpp/atlas_cell.h");
        /// Registers `AtlasCell` (`org.lmix.atlas`); before the engine.
        fn register_atlas_cell();
        /// A new frame of the tile picture: its GL texture, RGBA.
        fn atlas_frame(texture: u32, width: i32, height: i32);
        /// No tile picture any more.
        fn atlas_clear();
    }
}

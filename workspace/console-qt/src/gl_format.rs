//! The scene graph's OpenGL flavour (see `cpp/gl_format.h`).

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("console-qt/cpp/gl_format.h");
        /// Desktop OpenGL instead of OpenGL ES; before the application
        /// object.
        fn use_desktop_gl();
    }
}

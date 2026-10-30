//! Qt's QML profiler for measuring bindings (scripts/profile-gui.sh);
//! only in builds with the feature `qml-profile`.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("console-qt/cpp/qml_profile.h");
        /// Opens the profiler service on `port` for qmlprofiler to attach
        /// later; before the engine is made.
        fn start_qml_profiler(port: i32);
    }
}

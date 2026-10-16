//! Qt's own translations (`qtbase`, `qtdeclarative`); the Console's texts
//! come from `i18n/`.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("console-qt/cpp/qt_translations.h");
        /// Loads Qt's catalogs for the system language; needs the
        /// application object.
        fn install_qt_translations();
    }
}

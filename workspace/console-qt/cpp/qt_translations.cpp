#include "qt_translations.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QLibraryInfo>
#include <QtCore/QLocale>
#include <QtCore/QTranslator>

void install_qt_translations()
{
    auto *app = QCoreApplication::instance();
    const QString dir = QLibraryInfo::path(QLibraryInfo::TranslationsPath);
    // qtbase: QPlatformTheme's button texts; qtdeclarative: Qt Quick's.
    for (const char *catalog : {"qtbase", "qtdeclarative"}) {
        auto *translator = new QTranslator(app);
        if (translator->load(QLocale(), QString::fromLatin1(catalog), QStringLiteral("_"), dir))
            QCoreApplication::installTranslator(translator);
        else
            delete translator;
    }
}

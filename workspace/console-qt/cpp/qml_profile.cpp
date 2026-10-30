#include "console-qt/cpp/qml_profile.h"

#ifdef LMIX_QML_PROFILE
#include <QtQml/qqmldebug.h>
#endif

void start_qml_profiler(int port)
{
#ifdef LMIX_QML_PROFILE
    QQmlDebuggingEnabler::enableDebugging(true);
    QQmlDebuggingEnabler::setServices(QQmlDebuggingEnabler::profilerServices());
    QQmlDebuggingEnabler::startTcpDebugServer(port, QQmlDebuggingEnabler::DoNotWaitForClient);
#else
    (void)port;
#endif
}

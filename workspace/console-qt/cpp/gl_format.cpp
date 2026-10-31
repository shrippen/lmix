#include "gl_format.h"

#include <QtCore/QCoreApplication>
#include <QtGui/QSurfaceFormat>

void use_desktop_gl()
{
    QSurfaceFormat format = QSurfaceFormat::defaultFormat();
    format.setRenderableType(QSurfaceFormat::OpenGL);
    QSurfaceFormat::setDefaultFormat(format);
    // One share group for every window: a page in a window of its own
    // draws its input tiles from the main window's tile picture (one
    // texture, one stream; AtlasCell), not from a second stream.
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
}

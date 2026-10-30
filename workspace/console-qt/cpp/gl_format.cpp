#include "gl_format.h"

#include <QtGui/QSurfaceFormat>

void use_desktop_gl()
{
    QSurfaceFormat format = QSurfaceFormat::defaultFormat();
    format.setRenderableType(QSurfaceFormat::OpenGL);
    QSurfaceFormat::setDefaultFormat(format);
}

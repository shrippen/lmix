#include "atlas_cell.h"

#include <QtCore/QHash>
#include <QtCore/QMutex>
#include <QtCore/QPointer>
#include <QtCore/QCoreApplication>
#include <QtQml/qqml.h>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGImageNode>
#include <QtQuick/QSGTexture>

namespace {

struct Frame {
    std::uint32_t texture = 0;
    QSize size;
};

// Written by the bridge's streaming thread, read on the render thread
// during sync.
QMutex frameLock;
Frame current;

// GUI thread only.
QList<QPointer<AtlasCell>> &cells()
{
    static QList<QPointer<AtlasCell>> list;
    return list;
}

void updateCells()
{
    for (const auto &cell : cells()) {
        if (cell) {
            cell->update();
        }
    }
}

// Scene graph wrappers of the frames' textures (a pool of a few that
// GStreamer reuses), per window: a page in a window of its own draws the
// same texture (the windows' GL contexts share it, gl_format.cpp) through
// its own scene graph. Each window's are made, used and deleted on its
// render thread; with several windows there are several threads, hence
// the lock. The textures stay GStreamer's.
struct Wrappers {
    QSize size;
    QHash<std::uint32_t, QSGTexture *> textures;
};
QMutex wrapperLock;

QHash<QQuickWindow *, Wrappers> &wrappers()
{
    static QHash<QQuickWindow *, Wrappers> map;
    return map;
}

// The window's scene graph goes: its wrappers with it.
void dropWrappers(QQuickWindow *window)
{
    QMutexLocker locker(&wrapperLock);
    auto it = wrappers().find(window);
    if (it != wrappers().end()) {
        qDeleteAll(it->textures);
        wrappers().erase(it);
    }
}

QSGTexture *wrap(QQuickWindow *window, const Frame &frame)
{
    QMutexLocker locker(&wrapperLock);
    auto &own = wrappers()[window];
    if (frame.size != own.size) {
        qDeleteAll(own.textures);
        own.textures.clear();
        own.size = frame.size;
    }
    auto &map = own.textures;
    auto it = map.find(frame.texture);
    if (it == map.end()) {
        auto *texture = QNativeInterface::QSGOpenGLTexture::fromNative(
            frame.texture, window, frame.size, {});
        texture->setFiltering(QSGTexture::Linear);
        it = map.insert(frame.texture, texture);
    }
    return it.value();
}

} // namespace

AtlasCell::AtlasCell(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents);
    cells().append(this);
    connect(this, &QQuickItem::windowChanged, this, [](QQuickWindow *window) {
        // Once per window, however many cells it has.
        if (window && !window->property("lmixAtlasWatched").toBool()) {
            window->setProperty("lmixAtlasWatched", true);
            connect(window, &QQuickWindow::sceneGraphInvalidated, window,
                    [window] { dropWrappers(window); }, Qt::DirectConnection);
        }
    });
}

AtlasCell::~AtlasCell()
{
    cells().removeAll(this);
}

void AtlasCell::setCell(int cell)
{
    if (cell != m_cell) {
        m_cell = cell;
        Q_EMIT cellChanged();
        update();
    }
}

void AtlasCell::setColumns(int columns)
{
    columns = qMax(1, columns);
    if (columns != m_columns) {
        m_columns = columns;
        Q_EMIT gridChanged();
        update();
    }
}

void AtlasCell::setRows(int rows)
{
    rows = qMax(1, rows);
    if (rows != m_rows) {
        m_rows = rows;
        Q_EMIT gridChanged();
        update();
    }
}

QSGNode *AtlasCell::updatePaintNode(QSGNode *old, UpdatePaintNodeData *)
{
    Frame frame;
    {
        QMutexLocker locker(&frameLock);
        frame = current;
    }
    if (frame.texture == 0 || m_cell < 0 || m_cell >= m_columns * m_rows || width() <= 0
        || height() <= 0) {
        delete old;
        return nullptr;
    }
    auto *node = static_cast<QSGImageNode *>(old);
    if (!node) {
        node = window()->createImageNode();
        node->setOwnsTexture(false);
        node->setFiltering(QSGTexture::Linear);
    }
    node->setTexture(wrap(window(), frame));
    const qreal w = qreal(frame.size.width()) / m_columns;
    const qreal h = qreal(frame.size.height()) / m_rows;
    node->setSourceRect(QRectF((m_cell % m_columns) * w, (m_cell / m_columns) * h, w, h));
    node->setRect(boundingRect());
    return node;
}

void register_atlas_cell()
{
    qmlRegisterType<AtlasCell>("org.lmix.atlas", 1, 0, "AtlasCell");
}

void atlas_frame(std::uint32_t texture, std::int32_t width, std::int32_t height)
{
    {
        QMutexLocker locker(&frameLock);
        current = Frame{texture, QSize(width, height)};
    }
    QMetaObject::invokeMethod(QCoreApplication::instance(), updateCells, Qt::QueuedConnection);
}

void atlas_clear()
{
    atlas_frame(0, 0, 0);
}

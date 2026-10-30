#pragma once

#include <QtQuick/QQuickItem>
#include <cstdint>

// One cell of the tile picture (TilesAtlas): draws its part of the
// picture's GL texture directly, a quad with a source rectangle, so a
// page of input tiles costs one texture and no layer per tile. The
// frames come from the bridge (atlas_frame), the texture lives in the GL
// context the scene graph shares with GStreamer (qml6glsink's).
class AtlasCell : public QQuickItem
{
    Q_OBJECT
    /** The slot: cell `cell` of the grid, row by row; -1 for none. */
    Q_PROPERTY(int cell READ cell WRITE setCell NOTIFY cellChanged)
    Q_PROPERTY(int columns READ columns WRITE setColumns NOTIFY gridChanged)
    Q_PROPERTY(int rows READ rows WRITE setRows NOTIFY gridChanged)

public:
    explicit AtlasCell(QQuickItem *parent = nullptr);
    ~AtlasCell() override;

    int cell() const { return m_cell; }
    void setCell(int cell);
    int columns() const { return m_columns; }
    void setColumns(int columns);
    int rows() const { return m_rows; }
    void setRows(int rows);

Q_SIGNALS:
    void cellChanged();
    void gridChanged();

protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;

private:
    int m_cell = -1;
    int m_columns = 1;
    int m_rows = 1;
};

// Registers AtlasCell as `org.lmix.atlas` 1.0; before the engine loads.
void register_atlas_cell();
// A new frame: texture `texture` of `width` x `height` (RGBA), current in
// the context the scene graph shares; the bridge keeps it alive until three
// more came. Every cell draws it in the window's next frame.
void atlas_frame(std::uint32_t texture, std::int32_t width, std::int32_t height);
// No picture any more (released, broken): the cells draw nothing.
void atlas_clear();

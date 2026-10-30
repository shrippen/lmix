#pragma once

// Opens Qt's QML profiler service on port `port` for qmlprofiler
// (scripts/profile-gui.sh); built only with the cargo feature
// `qml-profile`, a no-op otherwise.
void start_qml_profiler(int port);

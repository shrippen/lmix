#pragma once

// Desktop OpenGL for the scene graph, not OpenGL ES: GStreamer's CUDA to
// GL copy (cudadownload, the live pictures from the engine's GPU) needs
// pixel buffer objects of desktop GL. And one GL share group for all
// windows (a page in its own window draws the tile picture too). Before
// the application object.
void use_desktop_gl();

#pragma once

// Desktop OpenGL for the scene graph, not OpenGL ES: GStreamer's CUDA to
// GL copy (cudadownload, the live pictures from the engine's GPU) needs
// pixel buffer objects of desktop GL. Before the application object.
void use_desktop_gl();

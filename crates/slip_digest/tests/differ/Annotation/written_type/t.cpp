// std: c++26
#include "side.h"
// §10.5 Annotation: `written_type`: the fact differs.
#ifdef B
[[=1L]] void f();
#else
[[=1]] void f();
#endif

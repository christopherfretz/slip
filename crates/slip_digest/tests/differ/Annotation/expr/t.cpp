// std: c++26
#include "side.h"
// §10.5 Annotation: `expr`: the fact differs.
#ifdef B
[[=1 + 0]] void f();
#else
[[=1]] void f();
#endif

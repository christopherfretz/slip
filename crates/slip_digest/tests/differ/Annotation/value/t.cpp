// std: c++26
#include "side.h"
// §10.5 Annotation: `value`: the fact differs.
#ifdef B
[[=2]] void f();
#else
[[=1]] void f();
#endif

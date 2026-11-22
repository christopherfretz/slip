// std: c++26
#include "side.h"
// §10.5 AnnotArena: `of_base`: the fact differs.
#ifdef B
struct Base {}; struct D : [[=1]] Base {};
#else
struct Base {}; struct D : Base {};
#endif

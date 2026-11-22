// std: c++26
#include "side.h"
// §10.5 AnnotArena: `of_decl`: the fact differs.
#ifdef B
[[=1]] void f();
#else
void f();
#endif

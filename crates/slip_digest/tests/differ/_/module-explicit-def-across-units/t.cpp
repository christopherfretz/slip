// module: m=m.cppm
// §10.7 ModuleTable `explicit_defs`: the explicit instantiation definition of a template of `m` in `m` itself (side A) against in the main interface unit `n` that imports it (side B).
module;
#include "side.h"
export module n;
import m;
#ifdef B
template void f<int>(int);
#endif

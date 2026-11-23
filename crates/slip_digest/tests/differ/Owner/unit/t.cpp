// module: m=m.cppm
// §10.7 Owner `unit`: one exported function owned by the imported interface `m` (side A) against owned by the main interface unit `n` (side B); both import `m`.
module;
#include "side.h"
export module n;
import m;
#ifdef B
export int f();
#endif

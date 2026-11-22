#include "side.h"
// §5.5 RecordData `lambda`: the closure type of `l` (a local type of its unit).
#ifdef B
auto l = [] { return 1L; };
#else
auto l = [] { return 0; };
#endif

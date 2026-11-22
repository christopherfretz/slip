#include "side.h"
// §5.5 RecordData `tag`: struct against union.
#ifdef B
union C {};
#else
struct C {};
#endif

#include "side.h"
// §7 GenericControl: `Type.0`: the fact differs.
#ifdef B
int v = _Generic(long, int: 1, long: 1);
#else
int v = _Generic(int, int: 1, long: 1);
#endif

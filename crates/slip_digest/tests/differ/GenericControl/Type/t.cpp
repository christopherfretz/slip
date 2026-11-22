#include "side.h"
// §7 GenericControl: `Type`: the fact differs.
int a = 1, b = 2, c = 3; long l;
#ifdef B
int v = _Generic(a, int: 1, default: 2);
#else
int v = _Generic(int, int: 1, default: 2);
#endif

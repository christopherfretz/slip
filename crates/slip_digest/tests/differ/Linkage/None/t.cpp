#include "side.h"
// §5.6 Linkage `None` (§1.7 row `enclosing`): an unnamed class that a typedef names for linkage purposes
// has linkage; one that the typedef does not name (it declares a pointer to it) has none.
#ifdef B
typedef struct { int a; } *T;
#else
typedef struct { int a; } T;
#endif

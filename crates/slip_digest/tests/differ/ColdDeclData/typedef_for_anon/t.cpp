#include "side.h"
// §5.7 ColdDeclData `typedef_for_anon` (keyed, digest §3.3): an unnamed class has the typedef name it is given for linkage, else its first declarator.
#ifdef B
typedef struct { int i; } *P, *T;
#else
typedef struct { int i; } *P, T;
#endif

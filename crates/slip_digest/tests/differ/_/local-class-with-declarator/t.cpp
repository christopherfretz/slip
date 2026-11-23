#include "side.h"
// §4.3: an unnamed local class in a DeclStmt with a declarator: its key is local(0).
#ifdef B
int f() { struct { long m; } s{1}; return (int)s.m; }
#else
int f() { struct { int m; } s{1}; return s.m; }
#endif

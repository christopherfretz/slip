#include "side.h"
// digest-kinds §2.4: a local class met again in its owner's stream (its
// definition after a declaration) writes its number alone; two such
// definitions swapped differ by those numbers only.
#ifdef B
int f() { struct P; struct Q; struct Q { int m; }; struct P { int m; }; P p{1}; Q q{2}; return p.m + q.m; }
#else
int f() { struct P; struct Q; struct P { int m; }; struct Q { int m; }; P p{1}; Q q{2}; return p.m + q.m; }
#endif

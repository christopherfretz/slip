#include "side.h"
// digest §4.3: the two unnamed local structs of an instantiation are
// numbered by its body (local(0), local(1)): their keys differ, and a
// change to the second is seen under its own key.
#ifdef B
template <class T> int f() { struct { int a = 1; } x; struct { long b = 3; } y; return x.a + int(y.b); }
#else
template <class T> int f() { struct { int a = 1; } x; struct { long b = 2; } y; return x.a + int(y.b); }
#endif
int z = f<int>();

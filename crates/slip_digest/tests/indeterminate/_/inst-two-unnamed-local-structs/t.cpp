#include "side.h"
// digest §4.3: the two unnamed local structs of an instantiation share one
// key and one name: indeterminate, never a silent merge.
template <class T> int f() { struct { int a = 1; } x; struct { long b = 2; } y; return x.a + int(y.b); }
int z = f<int>();

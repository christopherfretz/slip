#include "side.h"
// digest-kinds §11.1 property 7 for a default member initialiser written
// in source: no discriminator.
consteval int id(int x) { return x; }
struct S { int m = id(4); };
#ifdef B
void h() { S s; }
#endif

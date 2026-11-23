#include "side.h"
// digest-kinds rule §3.2: two conversion functions of one class.
struct S {
#ifdef B
operator int(); operator long();
#else
operator int();
#endif
};

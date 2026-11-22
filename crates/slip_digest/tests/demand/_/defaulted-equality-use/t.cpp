#include "side.h"
// digest §2.2: comparing with a defaulted operator== changes no unit.
struct S { int a; bool operator==(const S&) const = default; };
#ifdef B
bool use(const S& x) { return x == x; }
#else
bool use(const S& x) { return true; }
#endif

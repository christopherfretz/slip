#include "side.h"
// digest-kinds §11.9: a comparison defaulted in its class is defined on
// use.
struct S { int a; bool operator==(const S&) const = default; };
#ifdef B
bool h(const S& x) { return x == x; }
#endif

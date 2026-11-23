#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 5): the address of a
// static constexpr member of an implicit instantiation.
template <class T> struct S { static constexpr int v = sizeof(T); };
S<int> s;
#ifdef B
const int* use() { return &S<int>::v; }
#endif

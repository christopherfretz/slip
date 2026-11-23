#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 5): the own unit of a
// variable of the definition class reads no evaluation state.
template <class T> struct S { static constexpr int v = sizeof(T); };
S<int> s;
#ifdef B
void use() { static_assert(S<int>::v == 4); }
#endif

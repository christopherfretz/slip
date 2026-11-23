#include "side.h"
// digest §14 (WP-045 review probe): an exception specification Kiln
// resolves on use.
template <class T> struct X { void f() noexcept(sizeof(T) > 1) {} };
X<int> x;
#ifdef B
bool use() { return noexcept(x.f()); }
#endif

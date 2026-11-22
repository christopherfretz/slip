#include "side.h"
// §7 _: an earlier `noexcept` resolves `f<int>`'s deferred exception specification.
template <class T> void f() noexcept(sizeof(T) > 1);
#ifdef B
bool w = noexcept(f<int>());
#else

#endif
bool v = noexcept(f<int>());

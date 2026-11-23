#include "side.h"
// §4.4 "Spellings", decision 3: two functions with one `noexcept` profile in either order are each determinate and equal.
#ifdef B
namespace A2 { int f(int); template<class T> void h(T t) noexcept(noexcept(f(t))); }
namespace A1 { template<class T> void g(T t) noexcept(noexcept(f(t))); }
#else
namespace A1 { template<class T> void g(T t) noexcept(noexcept(f(t))); }
namespace A2 { int f(int); template<class T> void h(T t) noexcept(noexcept(f(t))); }
#endif

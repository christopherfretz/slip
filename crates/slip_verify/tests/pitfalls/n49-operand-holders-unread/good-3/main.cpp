#include "dep.h"

template <class T> int make(T);
template <class T, class U> concept Same = __is_same(T, U);
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };

namespace acme {
template <class T> void g(T) noexcept(true) {}
template void g<int>(int) noexcept(sizeof(legacy::make(0)) == 4);
}

#include "dep.h"

template <class T> int make(T);
template <class T, class U> concept Same = __is_same(T, U);
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };

namespace acme {
template <int N> int ft() { return N; }
template int ft<sizeof(legacy::make(0))>();
}

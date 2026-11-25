#include "dep.h"

template <class T> int make(T);
template <class T, class U> concept Same = __is_same(T, U);
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };

namespace legacy {
template <class T> int fb(T t) { decltype(t, W2<sizeof(make(0))>()) l{}; return 0; }
}

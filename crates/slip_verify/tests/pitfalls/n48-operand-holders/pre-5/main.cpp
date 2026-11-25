#include "dep.h"

template <class T> int make(T);
template <class T, class U> concept Same = __is_same(T, U);
template <class T> struct Wr { T m; };
template <int N> struct W2 { int f() { return N; } };

namespace legacy {
int va(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap, n); int r = __builtin_va_arg(ap, decltype(make(0))); __builtin_va_end(ap); return r; }
}

// std: c++26
#include "side.h"
// digest-kinds rule §3.3 `function`: `f(this T&&)` against `f(T&&)`, templates.
struct S {
#ifdef B
template <class T> void f(this T&&); template <class T> void f(T&&);
#else
template <class T> void f(T&&);
#endif
};

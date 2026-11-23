#include "side.h"
// §7 "Parentheses" (F-B): `return (x);` against `return x;` in a `decltype(auto)` function template: `f<int>` returns `int&` against `int`. The operand of `return` is the root of its walk.
int x;
#ifdef B
template<class T> decltype(auto) f(T) { return (x); }
#else
template<class T> decltype(auto) f(T) { return x; }
#endif

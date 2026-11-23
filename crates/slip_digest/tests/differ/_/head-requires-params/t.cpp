#include "side.h"
// §3.4, §4.4 item 6: a template head's requires-expression parameters are positional in the key: `x + 1` against `y + 1` is another constraint, a clean added/removed.
#ifdef B
template<class T> requires requires (T x, T y) { y + 1; } void f(T);
#else
template<class T> requires requires (T x, T y) { x + 1; } void f(T);
#endif

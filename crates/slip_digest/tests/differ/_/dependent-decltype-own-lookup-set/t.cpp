#include "side.h"
// §4.4 profile mode, F1: `h`'s `decltype(g(t))` has the profile of `f`'s, but lookup found `g(int)` alone on one side and both `g` on the other. The found set is not in the profile; the unit holds it from the spelling (WP-041).
int g(int);
template<class T> auto f(T t) -> decltype(g(t));
#ifdef B
template<class T> auto h(T t) -> decltype(g(t));
long g(long);
#else
long g(long);
template<class T> auto h(T t) -> decltype(g(t));
#endif

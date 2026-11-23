#include "side.h"
// §4.4 profile mode, F1: an unrelated `f` whose `decltype(g(t))` is spelled first, with another found set, is added; `h`'s bytes do not move.
int g(int);
#ifdef B
template<class T> auto f(T t) -> decltype(g(t));
#endif
long g(long);
template<class T> auto h(T t) -> decltype(g(t));

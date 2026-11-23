#include "side.h"
// §4.4 "Spellings", rule 11: a pseudo-destructor named by `decltype` (`p->~decltype(e)()`) in a body, `f(t)` against `h(t)`.
template<int N> struct Bq { static constexpr int value = 1; };
struct X {};
int f(int); int h(X);
#ifdef B
template<class T> void g(T t, int *p) { p->~decltype(int(Bq<sizeof(sizeof(h(t)))>::value))(); }
#else
template<class T> void g(T t, int *p) { p->~decltype(int(Bq<sizeof(sizeof(f(t)))>::value))(); }
#endif

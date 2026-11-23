#include "side.h"
// §4.4 "Spellings", rule 11: a `decltype` prefix in a member access in a body (`s.decltype(e)::value`), `f(t)` against `h(t)`.
template<int N> struct Bq { static constexpr int value = 1; };
struct S : Bq<8> {};
struct X {};
int f(int); int h(X);
#ifdef B
template<class T> int g(T t, S s) { return s.decltype(Bq<sizeof(sizeof(h(t)))>{})::value; }
#else
template<class T> int g(T t, S s) { return s.decltype(Bq<sizeof(sizeof(f(t)))>{})::value; }
#endif

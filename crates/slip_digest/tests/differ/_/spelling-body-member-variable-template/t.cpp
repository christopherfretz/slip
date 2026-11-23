#include "side.h"
// §4.4 "Spellings", rule 10: R2: the full walk: a member access (`MemberExpr`) in a function body to a static member variable template specialisation, its arguments `f` against `h`.
struct S { template<int N> static constexpr int vm = 1; };
int f(int); int h(int);
#ifdef B
template<class T> int g(T t, S s) { return s.vm<sizeof(sizeof(h(t)))>; }
#else
template<class T> int g(T t, S s) { return s.vm<sizeof(sizeof(f(t)))>; }
#endif

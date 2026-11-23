#include "side.h"
// §4.4 "Spellings", rule 10: R2: the full walk: a member access (`MemberExpr`) in a function body whose member is named through the qualifier, `f` against `h`.
template<int N> struct Bq { static constexpr int value = 1; };
struct S : Bq<8> {};
int f(int); int h(int);
#ifdef B
template<class T> int g(T t, S s) { return s.Bq<sizeof(sizeof(h(t)))>::value; }
#else
template<class T> int g(T t, S s) { return s.Bq<sizeof(sizeof(f(t)))>::value; }
#endif

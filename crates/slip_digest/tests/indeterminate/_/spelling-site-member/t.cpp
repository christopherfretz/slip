#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: as spelling-site-prefix, through a member access (`MemberExpr`) whose member is named through the qualifier.
template<int N> struct Bq { static constexpr int value = 1; };
struct S : Bq<8> {};
S s;
int f(int);
template<class T> void g(T, void (*)(decltype(T(s.Bq<sizeof(sizeof(f(T())))>::value))[2]));

#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: as spelling-site-prefix, through a member access (`MemberExpr`) whose member is named through the qualifier. Until WP-163 indeterminate: since Kiln WP-2720 (`dc5a7989e`) the parameter keeps its declared array type (`TypeView::Decayed`), whose spelling rule 2 reads; `f(long)` added changes the found set.
template<int N> struct Bq { static constexpr int value = 1; };
struct S : Bq<8> {};
S s;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, void (*)(decltype(T(s.Bq<sizeof(sizeof(f(T())))>::value))[2]));

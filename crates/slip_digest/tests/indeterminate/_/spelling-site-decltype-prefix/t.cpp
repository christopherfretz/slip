#include "side.h"
// §4.4 "Spellings", rule 5 and rule 11: as spelling-site-prefix, through a `decltype` prefix (a `DependentScopeDeclRefExpr` whose canonical qualifier is `Bq<8>`): the mark is raised at the qualifier, so the site is indeterminate.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
template<class T> void g(T, void (*)(decltype(T(decltype(Bq<sizeof(sizeof(f(T())))>{})::value))[2]));

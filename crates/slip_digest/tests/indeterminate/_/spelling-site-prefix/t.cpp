#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: a canonical `decltype` operand (an adjusted array of a function type that is not a declaration's own) holding a resolved name of a class member, which may have been named through a qualifier holding more: the mark is raised at the reference, so the site is indeterminate.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
template<class T> void g(T, void (*)(decltype(T(Bq<sizeof(sizeof(f(T())))>::value))[2]));

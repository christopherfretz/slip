#include "side.h"
// §4.4 "Spellings", rule 5 and rule 11: a dependent member access whose qualifier names a specialisation holding the found set of `f` (`T().Bq<..>::value`, spelled by name in profile mode), in a canonical `decltype` operand: the mark is raised at the qualifier, so the site is indeterminate.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
template<class T> void g(T, void (*)(decltype(T(T().Bq<sizeof(sizeof(f(T())))>::value))[2]));

#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: a canonical `decltype` operand (an adjusted array of a function type that is not a declaration's own) holding a resolved name of a class member, which may have been named through a qualifier holding more: the mark is raised at the reference, so the site is indeterminate. Until WP-163 indeterminate: since Kiln WP-2720 (`dc5a7989e`) the parameter keeps its declared array type (`TypeView::Decayed`), whose spelling rule 2 reads; `f(long)` added changes the found set.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, void (*)(decltype(T(Bq<sizeof(sizeof(f(T())))>::value))[2]));

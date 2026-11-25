#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: as spelling-site-prefix, through an enumerator (its context is the enumeration) named through the qualifier. Until WP-163 indeterminate: since Kiln WP-2720 (`dc5a7989e`) the parameter keeps its declared array type (`TypeView::Decayed`), whose spelling rule 2 reads; `f(long)` added changes the found set.
template<int N> struct Bq { enum { E1 = 1 }; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, void (*)(decltype(T(Bq<sizeof(sizeof(f(T())))>::E1))[2]));

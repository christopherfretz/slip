#include "side.h"
// §4.4 "Spellings", rule 5 and rule 10: as spelling-site-prefix, through an enumerator (its context is the enumeration) named through the qualifier.
template<int N> struct Bq { enum { E1 = 1 }; };
int f(int);
template<class T> void g(T, void (*)(decltype(T(Bq<sizeof(sizeof(f(T())))>::E1))[2]));

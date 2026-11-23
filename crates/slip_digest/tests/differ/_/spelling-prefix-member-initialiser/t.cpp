#include "side.h"
// §4.4 "Spellings", rule 10 (the review's t3): R2: the full walk: in a static member's initialiser.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct C { static constexpr int m = Bq<sizeof(sizeof(f(T())))>::value; };

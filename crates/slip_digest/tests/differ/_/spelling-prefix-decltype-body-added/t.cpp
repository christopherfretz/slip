#include "side.h"
// §4.4 "Spellings", rule 11 (the re-check's k01): a `decltype` prefix in a body, whose operand is instantiation-dependent: Kiln builds a `DependentScopeDeclRefExpr`, whose canonical qualifier is `Bq<8>`; side B adds `f(long)`, which the found set of `f` does not hold.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int g(T t) { return decltype(Bq<sizeof(sizeof(f(t)))>{})::value; }

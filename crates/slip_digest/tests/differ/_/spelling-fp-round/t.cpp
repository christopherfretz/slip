#include "side.h"
// §4.4 "Spellings", rule 8: the rounding mode where a floating operation of a profile was written is in the list; `h`'s body holds it too.
template<double D> struct A {};
#ifdef B
#pragma STDC FENV_ROUND FE_UPWARD
#endif
template<class T> auto g(T) -> A<(1.0 / 3.0) * sizeof(T)>;
template<class T> void h(T) { A<(1.0 / 3.0) * sizeof(T)> a; }

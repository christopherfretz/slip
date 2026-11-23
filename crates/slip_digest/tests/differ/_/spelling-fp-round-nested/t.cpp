#include "side.h"
// §4.4 "Spellings", rule 8: as `spelling-fp-round`, the operation nested.
template<double D> struct A {};
#ifdef B
#pragma STDC FENV_ROUND FE_UPWARD
#endif
template<class T> auto g(T) -> A<1.0 / (3.0 * sizeof(T))>;
